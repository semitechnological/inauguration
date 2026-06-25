//! Minimal native symbol resolver for JIT code.
//! Falls back to dlsym when function not in module map.

use std::collections::HashMap;
use std::sync::{Mutex, OnceLock};

#[derive(Clone, Copy)]
struct NativePtr(*const u8);
unsafe impl Send for NativePtr {}
unsafe impl Sync for NativePtr {}

fn cache() -> &'static Mutex<HashMap<String, NativePtr>> {
    static C: OnceLock<Mutex<HashMap<String, NativePtr>>> = OnceLock::new();
    C.get_or_init(|| Mutex::new(HashMap::new()))
}

pub fn resolve_native_fn(name: &str) -> Option<*const u8> {
    let c = cache().lock().unwrap();
    if let Some(np) = c.get(name) {
        return Some(np.0);
    }
    drop(c);

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

#[cfg(any(target_os = "macos", target_os = "linux", target_os = "android"))]
/// Pre-register critical libc symbols on init.
pub fn bootstrap_jit_native() {
    for name in &["system", "exit", "puts", "putchar", "printf"] {
        if let Some(ptr) = dlsym_exact(name) {
            cache()
                .lock()
                .unwrap()
                .insert(name.to_string(), NativePtr(ptr));
        }
    }
}

fn dlsym_exact(name: &str) -> Option<*const u8> {
    let c_name = std::ffi::CString::new(name).ok()?;
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
