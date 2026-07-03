//! C-ABI wrappers for std::env / std::fs helpers used by the native JIT path.
//!
//! These functions are called directly from AArch64 JIT code via the
//! pre-registered symbol table in `native_link`. They allocate returned
//! strings as length-prefixed UTF-8 blobs (8-byte little-endian length
//! followed by the bytes) so they match the format used by the in-runtime
//! string builtins. The JIT runtime does not currently free these blobs;
//! they are intentionally leaked for the lifetime of the process, matching
//! the existing behaviour of the in-runtime string helpers.
//!
//! # Safety
//!
//! Every exported function takes at least one raw pointer from JIT code.
//! The caller must ensure the pointer is valid, properly aligned for the
//! in-string header, and not aliased while the wrapper runs. All exported
//! functions are therefore `unsafe extern "C"`.

#[cfg(unix)]
use std::os::unix::ffi::OsStrExt;

/// In-runtime string layout: `len` (u64) followed by the UTF-8 bytes.
const INSTRING_LEN_SIZE: usize = 8;
const INSTRING_ALIGN: usize = 8;

unsafe fn instring_from_ptr(ptr: *const u8) -> Option<&'static [u8]> {
    if ptr.is_null() {
        return None;
    }
    unsafe {
        let len = *(ptr as *const u64);
        let data = ptr.add(INSTRING_LEN_SIZE);
        Some(std::slice::from_raw_parts(data, len as usize))
    }
}

unsafe fn instring_from_bytes(data: &[u8]) -> *const u8 {
    let total = INSTRING_LEN_SIZE + data.len();
    let layout = std::alloc::Layout::from_size_align(total, INSTRING_ALIGN)
        .expect("valid layout for instring allocation");
    let ptr = unsafe { std::alloc::alloc(layout) };
    if ptr.is_null() {
        std::alloc::handle_alloc_error(layout);
    }
    unsafe {
        *(ptr as *mut u64) = data.len() as u64;
        std::ptr::copy_nonoverlapping(
            data.as_ptr(),
            ptr.add(INSTRING_LEN_SIZE),
            data.len(),
        );
    }
    ptr
}

#[cfg(unix)]
unsafe fn instring_from_os_str(s: &std::ffi::OsStr) -> *const u8 {
    unsafe { instring_from_bytes(s.as_bytes()) }
}

#[cfg(not(unix))]
unsafe fn instring_from_os_str(s: &std::ffi::OsStr) -> *const u8 {
    unsafe { instring_from_bytes(s.to_str().unwrap_or("").as_bytes()) }
}

unsafe fn instring_from_string(s: String) -> *const u8 {
    unsafe { instring_from_bytes(s.as_bytes()) }
}

unsafe fn instring_empty() -> *const u8 {
    unsafe { instring_from_bytes(&[]) }
}

/// `std::env::var(key)` -> `Option<String>`
/// C-ABI: takes an instring pointer in X0, returns an instring pointer in X0
/// (empty string when the variable is missing, matching a `None` placeholder).
///
/// # Safety
/// `key_ptr` must be a valid, non-aliased instring pointer (8-byte aligned
/// header followed by `len` bytes) or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_env_var(key_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(key) = instring_from_ptr(key_ptr) else {
            return instring_empty();
        };
        let key_str = match std::str::from_utf8(key) {
            Ok(s) => s,
            Err(_) => return instring_empty(),
        };
        match std::env::var(key_str) {
            Ok(value) => instring_from_string(value),
            Err(_) => instring_empty(),
        }
    }
}

/// `std::env::temp_dir()` -> `PathBuf`
///
/// # Safety
/// No raw pointer arguments; safe to call from JIT with the standard C ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_env_temp_dir() -> *const u8 {
    unsafe { instring_from_os_str(std::env::temp_dir().as_os_str()) }
}

/// `std::env::current_dir()` -> `io::Result<PathBuf>`
///
/// # Safety
/// No raw pointer arguments; safe to call from JIT with the standard C ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_env_current_dir() -> *const u8 {
    unsafe {
        match std::env::current_dir() {
            Ok(path) => instring_from_os_str(path.as_os_str()),
            Err(_) => instring_empty(),
        }
    }
}

/// `std::fs::read_to_string(path)` -> `io::Result<String>`
///
/// # Safety
/// `path_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_fs_read_to_string(path_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(path_bytes) = instring_from_ptr(path_ptr) else {
            return instring_empty();
        };
        let path_str = match std::str::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return instring_empty(),
        };
        match std::fs::read_to_string(path_str) {
            Ok(content) => instring_from_string(content),
            Err(_) => instring_empty(),
        }
    }
}

/// `std::fs::exists(path)` -> `bool`
///
/// # Safety
/// `path_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_fs_exists(path_ptr: *const u8) -> i64 {
    unsafe {
        let Some(path_bytes) = instring_from_ptr(path_ptr) else {
            return 0;
        };
        let path_str = match std::str::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        if std::path::Path::new(path_str).exists() {
            1
        } else {
            0
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instring_roundtrip() {
        unsafe {
            let p = instring_from_bytes(b"hello");
            assert_eq!(instring_from_ptr(p).unwrap(), b"hello");
        }
    }

    #[test]
    fn in_env_temp_dir_returns_non_empty() {
        unsafe {
            let p = in_env_temp_dir();
            assert!(!p.is_null());
            let s = instring_from_ptr(p).unwrap();
            assert!(!s.is_empty());
        }
    }
}
