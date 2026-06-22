//! In-memory JIT execution runtime.
//!
//! Skips binary formats (Mach-O, ELF) entirely. Takes lowered AArch64/x86-64
//! machine code from `native_emit`, maps it into RWX pages via mmap, and
//! executes directly through function pointers.
//!
//! Architecture:
//!   Core IR → Machine IR → raw bytes → mmap(MAP_JIT) → call via fn ptr
//!
//! Systems-level: the JIT runtime IS the executable model. No files, no
//! linker, no dynamic loader. Functions resolve through an in-memory
//! dispatch table. Designed to eventually compile itself.

#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn sys_icache_invalidate(start: *const std::ffi::c_void, len: usize);
}

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::RwLock;

/// A compiled function resident in JIT memory.
struct JitFunction {
    /// Raw pointer to the first instruction in the JIT code page.
    entry: *const u8,
    /// Size of the function in bytes.
    size: u32,
    /// Stack frame size (for debugging / future stack walking).
    _frame_size: u32,
}

// SAFETY: JitFunction pointers reference mmap'd pages that remain valid for
// the lifetime of the JitRuntime. They are never aliased mutably after
// compilation.
unsafe impl Send for JitFunction {}
unsafe impl Sync for JitFunction {}

/// System-level JIT execution runtime.
///
/// Owns executable memory pages and a dispatch table. Functions are compiled
/// lazily: the first call to `invoke()` for a function triggers compilation
/// of the entire module, then all functions become callable.
pub struct JitRuntime {
    /// Mapped executable pages (the code arena).
    code_pages: Vec<CodePage>,
    /// Function dispatch table: name → entry point.
    functions: RwLock<HashMap<String, JitFunction>>,
}

struct CodePage {
    ptr: *mut u8,
    size: usize,
    used: usize,
}

// SAFETY: CodePage owns mmap'd memory that is valid until dropped.
unsafe impl Send for CodePage {}
unsafe impl Sync for CodePage {}

impl CodePage {
    fn new(min_size: usize) -> Option<Self> {
        let page_size = 0x4000; // 16KB on Apple ARM64
        let size = min_size.max(page_size).next_multiple_of(page_size);

        #[cfg(target_os = "macos")]
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_JIT,
                -1,
                0,
            )
        };

        #[cfg(not(target_os = "macos"))]
        let ptr = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                size,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };

        if ptr == libc::MAP_FAILED {
            return None;
        }

        #[cfg(not(target_os = "macos"))]
        unsafe {
            libc::mprotect(ptr, size, libc::PROT_READ | libc::PROT_EXEC);
        }

        Some(Self {
            ptr: ptr as *mut u8,
            size,
            used: 0,
        })
    }

    /// Make the written code executable and flush caches.
    fn finalize(&self) {
        #[cfg(target_os = "macos")]
        unsafe {
            // Flush instruction cache first
            sys_icache_invalidate(self.ptr as *const std::ffi::c_void, self.used);
            // Then make executable
            libc::mprotect(
                self.ptr as *mut std::ffi::c_void,
                self.size,
                libc::PROT_READ | libc::PROT_EXEC,
            );
        }
    }

    fn allocate(&mut self, len: usize) -> Option<*mut u8> {
        if self.used + len > self.size {
            return None;
        }
        let ptr = unsafe { self.ptr.add(self.used) };
        self.used += len;
        Some(ptr)
    }
}

impl Drop for CodePage {
    fn drop(&mut self) {
        unsafe {
            libc::munmap(self.ptr as *mut c_void, self.size);
        }
    }
}

impl JitRuntime {
    pub fn new() -> Self {
        Self {
            code_pages: Vec::new(),
            functions: RwLock::new(HashMap::new()),
        }
    }

    /// Load machine code bytes and register entry points.
    ///
    /// `code` is raw AArch64 machine instructions. `entry_offset` is the
    /// byte offset from the start of `code` to the entry trampoline.
    /// `function_offsets` maps function names to (offset, size) pairs.
    pub fn load(
        &mut self,
        code: &[u8],
        function_offsets: &[(String, u32, u32)], // (name, offset, size)
    ) -> Result<(), String> {
        // Allocate a code page large enough
        let page = CodePage::new(code.len())
            .ok_or_else(|| "jit: mmap failed".to_string())?;

        let dest = page.ptr;
        unsafe {
            std::ptr::copy_nonoverlapping(code.as_ptr(), dest, code.len());
        }

        // On Apple ARM64, flush the instruction cache after writing code
        self.code_pages.push(page);

        // Make code pages executable
        let last_page = self.code_pages.last().unwrap();
        last_page.finalize();

        let base = self.code_pages.last().unwrap().ptr;
        let mut funcs = self.functions.write().unwrap();
        for (name, offset, size) in function_offsets {
            funcs.insert(
                name.clone(),
                JitFunction {
                    entry: unsafe { base.add(*offset as usize) },
                    size: *size,
                    _frame_size: 0,
                },
            );
        }

        Ok(())
    }

    /// Call a compiled function by name.
    ///
    /// # Safety
    /// The function must have been loaded via `load()`. The caller is
    /// responsible for ensuring the function signature matches the arguments.
    pub unsafe fn invoke(&self, _name: &str, _args: &[i64]) -> Option<i64> {
        let funcs = self.functions.read().unwrap();
        let func = funcs.get(_name)?;
        let entry = func.entry as *const ();

        // Call the function through a raw function pointer.
        // AArch64 ABI: first 8 integer args in x0-x7, return in x0.
        // For now, we support up to 6 args + return i64.
        match _args.len() {
            0 => {
                let f: extern "C" fn() -> i64 = std::mem::transmute(entry);
                Some(f())
            }
            1 => {
                let f: extern "C" fn(i64) -> i64 = std::mem::transmute(entry);
                Some(f(_args[0]))
            }
            2 => {
                let f: extern "C" fn(i64, i64) -> i64 = std::mem::transmute(entry);
                Some(f(_args[0], _args[1]))
            }
            3 => {
                let f: extern "C" fn(i64, i64, i64) -> i64 = std::mem::transmute(entry);
                Some(f(_args[0], _args[1], _args[2]))
            }
            4 => {
                let f: extern "C" fn(i64, i64, i64, i64) -> i64 = std::mem::transmute(entry);
                Some(f(_args[0], _args[1], _args[2], _args[3]))
            }
            5 => {
                let f: extern "C" fn(i64, i64, i64, i64, i64) -> i64 = std::mem::transmute(entry);
                Some(f(_args[0], _args[1], _args[2], _args[3], _args[4]))
            }
            6 => {
                let f: extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64 =
                    std::mem::transmute(entry);
                Some(f(
                    _args[0], _args[1], _args[2], _args[3], _args[4], _args[5],
                ))
            }
            _ => None,
        }
    }

    /// Returns the number of loaded functions.
    pub fn function_count(&self) -> usize {
        self.functions.read().unwrap().len()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn jit_runtime_new_is_empty() {
        let rt = JitRuntime::new();
        assert_eq!(rt.function_count(), 0);
    }
}
