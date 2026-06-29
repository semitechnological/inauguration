//! In-memory JIT execution runtime.
//!
//! Skips binary formats (Mach-O, ELF) entirely. Takes lowered AArch64/x86-64
//! machine code from `native_emit`, maps it into executable memory via
//! mmap (Unix) or VirtualAlloc (Windows), and executes directly through
//! function pointers.
//!
//! Architecture:
//!   Core IR → Machine IR → raw bytes → mmap/VirtualAlloc → call via fn ptr
//!
//! Systems-level: the JIT runtime IS the executable model. No files, no
//! linker, no dynamic loader. Functions resolve through an in-memory
//! dispatch table. Designed to eventually compile itself.

use std::collections::HashMap;
use std::ffi::c_void;
use std::sync::RwLock;

// SAFETY: These are thin FFI wrappers around macOS kernel APIs.
// sys_icache_invalidate takes an arbitrary pointer and length; caller
// must ensure the range is within a valid mapped page.
#[cfg(target_os = "macos")]
unsafe extern "C" {
    fn sys_icache_invalidate(start: *const std::ffi::c_void, len: usize);
}

// SAFETY: pthread_jit_write_protect_np is available on macOS 11.0+
// through libSystem (always linked). It toggles write protection on
// MAP_JIT pages. Calling with 0 enables writing (disables execution),
// calling with 1 enables execution (disables writing).
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
unsafe extern "C" {
    fn pthread_jit_write_protect_np(enabled: std::ffi::c_int);
}

/// A compiled function resident in JIT memory.
struct JitFunction {
    /// Raw pointer to the first instruction in the JIT code page.
    entry: *const u8,
    /// Size of the function in bytes.
    size: u32,
    /// Stack frame size (for debugging / future stack walking).
    _frame_size: u32,
}

// SAFETY: JitFunction stores raw pointers to mmap'd pages owned by JitRuntime.
// These pages remain valid for the lifetime of JitRuntime and are never
// mutably aliased after compilation completes. JitFunction is only accessed
// through &self methods on JitRuntime which holds a RwLock.
unsafe impl Send for JitFunction {}
// SAFETY: Same rationale as Send. JitFunction is only read (entry pointer,
// size) and never mutated after construction. Concurrent reads through
// RwLock are safe.
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
    /// Writable page for error flag (byte at +0) and error value (ptr at +8).
    /// Set by invoke() into X27 before calling JIT code. Throw/try use this.
    error_page: *mut u8,
}

struct CodePage {
    ptr: *mut u8,
    size: usize,
    used: usize,
}

// SAFETY: CodePage owns unique ownership of a single mmap'd region.
// It is only accessed through &mut self methods (allocate) or &self
// (finalize, which is called once). Drop cleans up the mapping.
unsafe impl Send for CodePage {}
// SAFETY: CodePage is only read after finalize(). No mutation races
// because the JitRuntime holds a Vec<CodePage> behind &mut self during
// load() and only reads code_pages.last() during invoke().
unsafe impl Sync for CodePage {}

// SAFETY: mmap with MAP_JIT (macOS) or MAP_ANON (Linux) creates anonymous
// writable pages. MAP_JIT pages on Apple Silicon require
// pthread_jit_write_protect_np to toggle execution; on Intel macOS,
// mprotect(PROT_EXEC) works. On Linux, mprotect(PROT_EXEC) is the standard
// path. Caller must munmap on drop.
#[cfg(not(windows))]
fn alloc_executable_pages(size: usize) -> Option<*mut u8> {
    #[cfg(target_os = "macos")]
    let flags = libc::MAP_PRIVATE | libc::MAP_ANON | libc::MAP_JIT;
    #[cfg(not(target_os = "macos"))]
    let flags = libc::MAP_PRIVATE | libc::MAP_ANON;
    // SAFETY: mmap is safe per POSIX. Returns MAP_FAILED on error.
    // Address hint is null (kernel chooses). File descriptor -1 with
    // MAP_ANON means no backing file.
    let ptr = unsafe {
        libc::mmap(
            std::ptr::null_mut(),
            size,
            libc::PROT_READ | libc::PROT_WRITE,
            flags,
            -1,
            0,
        )
    };
    if ptr == libc::MAP_FAILED {
        return None;
    }
    Some(ptr as *mut u8)
}

#[cfg(not(windows))]
fn make_executable(_ptr: *mut u8, _size: usize) {
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    // SAFETY: On Apple Silicon, MAP_JIT pages use pthread_jit_write_protect_np
    // to toggle execution. Calling with enabled=1 disables writing and enables
    // execution. This is the correct way to transition MAP_JIT pages to RX
    // on arm64 macOS; mprotect does not work reliably for this purpose.
    unsafe {
        pthread_jit_write_protect_np(1);
    }
    #[cfg(not(all(target_os = "macos", target_arch = "aarch64")))]
    // SAFETY: mprotect changes page protection. ptr must point to a mapped
    // page boundary (page-aligned). size should cover all pages to protect.
    // This is safe because we only call it on pages we own via mmap.
    unsafe {
        libc::mprotect(
            ptr as *mut std::ffi::c_void,
            size,
            libc::PROT_READ | libc::PROT_EXEC,
        );
    }
}

// SAFETY: On Apple Silicon, before writing to a MAP_JIT page we must call
// pthread_jit_write_protect_np(0) to enable writing. This disables execution
// on that thread until the next call with enabled=1.
#[cfg(all(target_os = "macos", target_arch = "aarch64"))]
fn make_writable_for_jit() {
    unsafe {
        pthread_jit_write_protect_np(0);
    }
}

#[cfg(not(windows))]
fn free_pages(ptr: *mut u8, size: usize) {
    unsafe {
        libc::munmap(ptr as *mut c_void, size);
    }
}

#[cfg(windows)]
fn alloc_executable_pages(size: usize) -> Option<*mut u8> {
    const MEM_RESERVE: u32 = 0x2000;
    const MEM_COMMIT: u32 = 0x1000;
    const PAGE_EXECUTE_READWRITE: u32 = 0x40;
    unsafe extern "system" {
        fn VirtualAlloc(
            lp_address: *mut std::ffi::c_void,
            dw_size: usize,
            fl_allocation_type: u32,
            fl_protect: u32,
        ) -> *mut std::ffi::c_void;
    }
    let ptr = unsafe {
        VirtualAlloc(
            std::ptr::null_mut(),
            size,
            MEM_RESERVE | MEM_COMMIT,
            PAGE_EXECUTE_READWRITE,
        )
    };
    if ptr.is_null() {
        return None;
    }
    Some(ptr as *mut u8)
}

#[cfg(windows)]
fn make_executable(_ptr: *mut u8, _size: usize) {
    // Already RWX from VirtualAlloc. ARM64 Windows needs FlushInstructionCache
    // but that requires win32_ffi we can add when tested.
}

#[cfg(windows)]
fn free_pages(ptr: *mut u8, _size: usize) {
    const MEM_RELEASE: u32 = 0x8000;
    unsafe extern "system" {
        fn VirtualFree(lp_address: *mut std::ffi::c_void, dw_size: usize, dw_free_type: u32)
        -> i32;
    }
    unsafe {
        VirtualFree(ptr as *mut std::ffi::c_void, 0, MEM_RELEASE);
    }
}

impl CodePage {
    fn new(min_size: usize) -> Option<Self> {
        let page_size = 0x4000; // 16KB typical for ARM64, works for x86_64 too
        let size = min_size.max(page_size).next_multiple_of(page_size);

        let ptr = alloc_executable_pages(size)?;

        // SAFETY: On Apple Silicon, MAP_JIT pages start in executable state.
        // We must toggle to writable before copying code into them.
        #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
        make_writable_for_jit();

        Some(Self {
            ptr: ptr as *mut u8,
            size,
            used: 0,
        })
    }

    /// Make the written code executable and flush caches.
    fn finalize(&self) {
        #[cfg(target_os = "macos")]
        // SAFETY: sys_icache_invalidate ensures icache coherence after
        // self-modifying code. ptr and used are within the mapped region.
        unsafe {
            sys_icache_invalidate(self.ptr as *const std::ffi::c_void, self.used);
        }
        #[cfg(all(
            any(target_os = "linux", target_os = "android"),
            target_arch = "aarch64"
        ))]
        // SAFETY: __clear_cache is a GCC built-in that flushes the
        // instruction cache for the given range. ptr through ptr+used
        // is within a valid mapped page.
        unsafe {
            extern "C" {
                fn __clear_cache(start: *const u8, end: *const u8);
            }
            __clear_cache(self.ptr, self.ptr.add(self.used));
        }
        // SAFETY: make_executable uses pthread_jit_write_protect_np on
        // Apple Silicon or mprotect on other Unix. Both transition the
        // page from writable to executable after code has been written
        // and caches flushed.
        make_executable(self.ptr, self.size);
    }

    fn allocate(&mut self, len: usize) -> Option<*mut u8> {
        if self.used + len > self.size {
            return None;
        }
        // SAFETY: ptr.add(self.used) stays within the mapped region because
        // we check used + len <= size first. The returned pointer is used
        // for writing instructions, then the page is made executable.
        let ptr = unsafe { self.ptr.add(self.used) };
        self.used += len;
        Some(ptr)
    }
}

impl Drop for CodePage {
    fn drop(&mut self) {
        free_pages(self.ptr, self.size);
    }
}

impl Default for JitRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl JitRuntime {
    pub fn new() -> Self {
        // Bootstrap native symbol cache (safe IO-only functions, NOT system())
        crate::native_emit::native_link::bootstrap_jit_native();
        // Allocate a small writable page for error flag/value.
        // Not MAP_JIT — just RW for throw/try/catch writes.
        // SAFETY: mmap with PROT_READ|PROT_WRITE (no MAP_JIT) is a standard
        // anonymous allocation. 64 bytes is well within page size. Error page
        // is never made executable.
        let error_page = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                64,
                libc::PROT_READ | libc::PROT_WRITE,
                libc::MAP_PRIVATE | libc::MAP_ANON,
                -1,
                0,
            )
        };
        let error_page = if error_page == libc::MAP_FAILED {
            std::ptr::null_mut()
        } else {
            error_page as *mut u8
        };
        Self {
            code_pages: Vec::new(),
            functions: RwLock::new(HashMap::new()),
            error_page,
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
        relocations: &[(u32, u64)],              // (offset, codegen_base) — patched at load time
    ) -> Result<(), String> {
        // Allocate a code page large enough
        let page = CodePage::new(code.len()).ok_or_else(|| "jit: mmap failed".to_string())?;

        let dest = page.ptr;
        // SAFETY: ptr::copy_nonoverlapping requires that src and dst are
        // valid for code.len() bytes and do not overlap. dest is a freshly
        // mmap'd page, code is a &[u8] pointing to Rust-owned memory.
        // These are guaranteed non-overlapping.
        unsafe {
            std::ptr::copy_nonoverlapping(code.as_ptr(), dest, code.len());
        }

        // Apply relocations: patch each absolute address by adding (actual_base - codegen_base)
        if !relocations.is_empty() {
            let actual_base = dest as u64;
            for &(offset, codegen_base) in relocations {
                let site = offset as usize;
                // Check bounds: relocation writes 8 bytes at offset
                if site + 8 <= code.len() {
                    // SAFETY: We verified site+8 <= code.len() and dest is
                    // a valid writable page of at least code.len() bytes.
                    // Reading from code[site..site+8] is within the input
                    // slice. Writing to dest.add(site) is within the page.
                    let old_val = u64::from_le_bytes([
                        code[site],
                        code[site + 1],
                        code[site + 2],
                        code[site + 3],
                        code[site + 4],
                        code[site + 5],
                        code[site + 6],
                        code[site + 7],
                    ]);
                    let new_val = old_val.wrapping_sub(codegen_base).wrapping_add(actual_base);
                    let patch = new_val.to_le_bytes();
                    unsafe {
                        std::ptr::copy_nonoverlapping(patch.as_ptr(), dest.add(site), 8);
                    }
                }
            }
        }

        self.code_pages.push(page);

        // Make code pages executable after writing
        let last_page = self.code_pages.last().unwrap();
        last_page.finalize();

        let base = self.code_pages.last().unwrap().ptr;
        let mut funcs = self.functions.write().unwrap();
        for (name, offset, size) in function_offsets {
            // SAFETY: offset is validated by the caller to be within
            // code.len(). base is the start of the mmap'd page containing
            // code. base.add(offset) is within the mapped region.
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
    pub unsafe fn invoke(&self, name: &str, _args: &[i64]) -> Option<i64> {
        let funcs = self.functions.read().unwrap();
        let func = funcs.get(name)?;
        let entry = func.entry as *const ();

        #[cfg(target_arch = "aarch64")]
        {
            // Set X27 to error page for throw/try/catch, then call through blr.
            let ep = self.error_page as usize;
            let result = match _args.len() {
                0 => {
                    let r: i64;
                    // SAFETY: The JIT function was loaded via load() and uses
                    // the standard C ABI. entry points to executable code in
                    // a MAP_JIT page. blr clobbers all caller-save registers
                    // per the C ABI (clobber_abi("C")). X27 is set for error
                    // page access — JIT functions read it in their prologue.
                    unsafe {
                        std::arch::asm!(
                            "mov x27, {e}",
                            "blr {f}",
                            e = in(reg) ep,
                            f = in(reg) entry,
                            lateout("x0") r,
                            clobber_abi("C"),
                        );
                    }
                    r
                }
                1 => {
                    let a0 = _args[0];
                    let r: i64;
                    // SAFETY: Same as 0-arg case, with x0 = first argument
                    // per the AArch64 C ABI (arg in x0, return in x0).
                    unsafe {
                        std::arch::asm!(
                            "mov x27, {e}",
                            "blr {f}",
                            e = in(reg) ep,
                            f = in(reg) entry,
                            in("x0") a0,
                            lateout("x0") r,
                            clobber_abi("C"),
                        );
                    }
                    r
                }
                2 => {
                    let a0 = _args[0];
                    let a1 = _args[1];
                    let r: i64;
                    // SAFETY: Same as 1-arg case, with x0 = arg0, x1 = arg1
                    // per the AArch64 C ABI.
                    unsafe {
                        std::arch::asm!(
                            "mov x27, {e}",
                            "blr {f}",
                            e = in(reg) ep,
                            f = in(reg) entry,
                            in("x0") a0,
                            in("x1") a1,
                            lateout("x0") r,
                            clobber_abi("C"),
                        );
                    }
                    r
                }
                _ => 0,
            };
            Some(result)
        }

        #[cfg(target_arch = "x86_64")]
        {
            // System V AMD64 ABI: args in rdi, rsi, rdx, rcx, r8, r9; return in rax.
            // SAFETY: Transmute converts a *const () (validated entry point from
            // a MAP_ANON|PROT_EXEC page) to a typed function pointer. The caller
            // must ensure the JIT-compiled function actually expects 6 i64 params
            // per the sysv64 ABI. JIT functions use a flat arg-passing convention.
            type JitFn = unsafe extern "C" fn(i64, i64, i64, i64, i64, i64) -> i64;
            let f: JitFn = unsafe { std::mem::transmute::<*const (), JitFn>(entry) };
            let result = match _args.len() {
                // SAFETY: Calling through transmuted function pointer. Standard
                // C ABI with zero-padded args (unused params are ignored by
                // the JIT function).
                0 => unsafe { f(0, 0, 0, 0, 0, 0) },
                1 => unsafe { f(_args[0], 0, 0, 0, 0, 0) },
                2 => unsafe { f(_args[0], _args[1], 0, 0, 0, 0) },
                _ => 0,
            };
            Some(result)
        }
    }

    /// Returns the number of loaded functions.
    pub fn function_count(&self) -> usize {
        self.functions.read().unwrap().len()
    }
}

impl Drop for JitRuntime {
    fn drop(&mut self) {
        if !self.error_page.is_null() {
            // SAFETY: error_page was allocated via mmap in new().
            // 64 bytes is the allocated size. The pointer is not null
            // (checked above) and no other code references it after drop.
            unsafe {
                libc::munmap(self.error_page as *mut c_void, 64);
            }
        }
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
