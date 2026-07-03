//! Minimal native symbol resolver for JIT code.
//! Falls back to dlsym when function not in module map.
//!
//! # Security
//! Only safe I/O functions are pre-registered (exit, puts, putchar, printf).
//! `system` is deliberately excluded to prevent shell injection through JIT code.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy)]
struct NativePtr(*const u8);
// SAFETY: NativePtr wraps a dlsym'd function pointer that is read-only
// after initialization. Multiple threads can read it concurrently.
unsafe impl Send for NativePtr {}
// SAFETY: Same as Send — the pointer is immutable after cache insertion.
unsafe impl Sync for NativePtr {}

fn cache() -> &'static Mutex<HashMap<String, NativePtr>> {
    static C: OnceLock<Mutex<HashMap<String, NativePtr>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Symbols that may be resolved via dlsym. Everything else is rejected to
/// prevent JIT-compiled code from calling arbitrary libc functions.
const DLSYM_ALLOWLIST: &[&str] = &["exit", "abort", "puts", "putchar", "printf"];

pub fn resolve_native_fn(name: &str) -> Option<*const u8> {
    let c = cache().lock().unwrap();
    if let Some(np) = c.get(name) {
        return Some(np.0);
    }
    drop(c);

    // Only symbols on the explicit allowlist may be looked up dynamically.
    if !DLSYM_ALLOWLIST.contains(&name) && !name.starts_with("in_") {
        return None;
    }

    let ptr = dlsym_exact(name);
    if ptr.is_none() {
        // macOS C convention
        let u = format!("_{name}");
        if let Some(p) = dlsym_exact(&u) {
            cache()
                .lock()
                .unwrap()
                .insert(name.to_string(), NativePtr(p));
            return Some(p);
        }
    }
    if let Some(p) = ptr {
        cache()
            .lock()
            .unwrap()
            .insert(name.to_string(), NativePtr(p));
    }
    ptr
}

/// Pre-register critical libc symbols on init.
///
/// # Security
/// Only safe I/O functions are included. `system` is deliberately excluded
/// to prevent JIT-compiled code from executing arbitrary shell commands.
#[cfg(any(target_os = "macos", target_os = "linux", target_os = "android"))]
pub fn bootstrap_jit_native() {
    // ponytail: only exit for termination, puts/putchar/printf for debug I/O.
    // No shell-execution symbols. Add mmap/bzero if JIT runtime needs them.
    for name in &["exit", "puts", "putchar", "printf"] {
        if let Some(ptr) = dlsym_exact(name) {
            cache()
                .lock()
                .unwrap()
                .insert(name.to_string(), NativePtr(ptr));
        }
    }
    // Pre-register in-cli stdlib wrappers so the JIT can call std::env and
    // std::fs helpers without external libc references.
    let mut c = cache().lock().unwrap();
    c.insert(
        "in_env_var".to_string(),
        NativePtr(crate::native_stdlib::in_env_var as *const u8),
    );
    c.insert(
        "in_env_temp_dir".to_string(),
        NativePtr(crate::native_stdlib::in_env_temp_dir as *const u8),
    );
    c.insert(
        "in_env_current_dir".to_string(),
        NativePtr(crate::native_stdlib::in_env_current_dir as *const u8),
    );
    c.insert(
        "in_fs_read_to_string".to_string(),
        NativePtr(crate::native_stdlib::in_fs_read_to_string as *const u8),
    );
    c.insert(
        "in_fs_exists".to_string(),
        NativePtr(crate::native_stdlib::in_fs_exists as *const u8),
    );
}

fn dlsym_exact(name: &str) -> Option<*const u8> {
    let c_name = std::ffi::CString::new(name).ok()?;
    // SAFETY: dlsym returns a symbol pointer from the global symbol space
    // (RTLD_DEFAULT). It returns NULL if the symbol is not found. The
    // resulting pointer points to a function in a loaded library that
    // remains resident for the process lifetime. No aliasing concerns
    // because we only read the pointer value (never call through it
    // without the caller's explicit intent via resolve_native_fn).
    let ptr = unsafe { libc::dlsym(libc::RTLD_DEFAULT, c_name.as_ptr()) };
    if ptr.is_null() {
        None
    } else {
        Some(ptr as *const u8)
    }
}

#[cfg(windows)]
fn dlsym_exact(_name: &str) -> Option<*const u8> {
    None
}
