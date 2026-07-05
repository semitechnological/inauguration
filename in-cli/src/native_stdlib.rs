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
    if ptr.is_null() || (ptr as usize) % INSTRING_ALIGN != 0 {
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
        std::ptr::copy_nonoverlapping(data.as_ptr(), ptr.add(INSTRING_LEN_SIZE), data.len());
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

/// `std::fs::write(path, contents)` -> success flag
/// Returns 1 on success, 0 on error.
///
/// # Safety
/// Both pointers must be valid, non-aliased instring pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_fs_write(path_ptr: *const u8, contents_ptr: *const u8) -> i64 {
    unsafe {
        let Some(path_bytes) = instring_from_ptr(path_ptr) else {
            return 0;
        };
        let Some(contents) = instring_from_ptr(contents_ptr) else {
            return 0;
        };
        let path_str = match std::str::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        match std::fs::write(path_str, contents) {
            Ok(()) => 1,
            Err(_) => 0,
        }
    }
}

/// `std::fs::create_dir(path)` -> success flag
/// Returns 1 on success, 0 on error.
///
/// # Safety
/// `path_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_fs_create_dir(path_ptr: *const u8) -> i64 {
    unsafe {
        let Some(path_bytes) = instring_from_ptr(path_ptr) else {
            return 0;
        };
        let path_str = match std::str::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        match std::fs::create_dir(path_str) {
            Ok(()) => 1,
            Err(_) => 0,
        }
    }
}

/// `std::fs::remove_file(path)` -> success flag
/// Returns 1 on success, 0 on error.
///
/// # Safety
/// `path_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_fs_remove_file(path_ptr: *const u8) -> i64 {
    unsafe {
        let Some(path_bytes) = instring_from_ptr(path_ptr) else {
            return 0;
        };
        let path_str = match std::str::from_utf8(path_bytes) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        match std::fs::remove_file(path_str) {
            Ok(()) => 1,
            Err(_) => 0,
        }
    }
}

/// `std::env::set_var(key, value)`
///
/// # Safety
/// Both pointers must be valid, non-aliased instring pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_env_set_var(key_ptr: *const u8, value_ptr: *const u8) {
    unsafe {
        let Some(key) = instring_from_ptr(key_ptr) else {
            return;
        };
        let Some(value) = instring_from_ptr(value_ptr) else {
            return;
        };
        let key_str = match std::str::from_utf8(key) {
            Ok(s) => s,
            Err(_) => return,
        };
        let value_str = match std::str::from_utf8(value) {
            Ok(s) => s,
            Err(_) => return,
        };
        std::env::set_var(key_str, value_str);
    }
}

/// `std::env::remove_var(key)`
///
/// # Safety
/// `key_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_env_remove_var(key_ptr: *const u8) {
    unsafe {
        let Some(key) = instring_from_ptr(key_ptr) else {
            return;
        };
        let key_str = match std::str::from_utf8(key) {
            Ok(s) => s,
            Err(_) => return,
        };
        std::env::remove_var(key_str);
    }
}

/// `String::contains(&self, pattern)` -> `bool`
/// Returns 1 if `self` contains `pattern`, 0 otherwise.
///
/// # Safety
/// Both pointers must be valid, non-aliased instring pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_contains(self_ptr: *const u8, pattern_ptr: *const u8) -> i64 {
    unsafe {
        let Some(self_bytes) = instring_from_ptr(self_ptr) else {
            return 0;
        };
        let Some(pattern_bytes) = instring_from_ptr(pattern_ptr) else {
            return 0;
        };
        let self_str = match std::str::from_utf8(self_bytes) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let pattern_str = match std::str::from_utf8(pattern_bytes) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        if self_str.contains(pattern_str) { 1 } else { 0 }
    }
}

/// `String::starts_with(&self, pattern)` -> `bool`
///
/// # Safety
/// Both pointers must be valid, non-aliased instring pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_starts_with(self_ptr: *const u8, pattern_ptr: *const u8) -> i64 {
    unsafe {
        let Some(self_bytes) = instring_from_ptr(self_ptr) else {
            return 0;
        };
        let Some(pattern_bytes) = instring_from_ptr(pattern_ptr) else {
            return 0;
        };
        let self_str = match std::str::from_utf8(self_bytes) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let pattern_str = match std::str::from_utf8(pattern_bytes) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        if self_str.starts_with(pattern_str) {
            1
        } else {
            0
        }
    }
}

/// `String::concat(a, b)` -> `String`
/// Concatenates two instring values and returns a newly allocated instring.
///
/// # Safety
/// Both pointers must be valid, non-aliased instring pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_concat(a_ptr: *const u8, b_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(a) = instring_from_ptr(a_ptr) else {
            return instring_empty();
        };
        let Some(b) = instring_from_ptr(b_ptr) else {
            return instring_from_bytes(a);
        };
        let mut out = Vec::with_capacity(a.len() + b.len());
        out.extend_from_slice(a);
        out.extend_from_slice(b);
        instring_from_bytes(&out)
    }
}

/// `print(text)` -> void
/// Prints the instring to stdout without a trailing newline.
///
/// # Safety
/// `text_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_print(text_ptr: *const u8) {
    unsafe {
        if let Some(text) = instring_from_ptr(text_ptr) {
            if let Ok(s) = std::str::from_utf8(text) {
                print!("{}", s);
            }
        }
    }
}

/// `print_int(n)` -> void
/// Prints the signed integer to stdout without a trailing newline.
///
/// # Safety
/// No raw pointer arguments; safe to call from JIT with the standard C ABI.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_print_int(n: i64) {
    print!("{}", n);
}

/// `String::ends_with(&self, pattern)` -> `bool`
///
/// # Safety
/// Both pointers must be valid, non-aliased instring pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_ends_with(self_ptr: *const u8, pattern_ptr: *const u8) -> i64 {
    unsafe {
        let Some(self_bytes) = instring_from_ptr(self_ptr) else {
            return 0;
        };
        let Some(pattern_bytes) = instring_from_ptr(pattern_ptr) else {
            return 0;
        };
        let self_str = match std::str::from_utf8(self_bytes) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        let pattern_str = match std::str::from_utf8(pattern_bytes) {
            Ok(s) => s,
            Err(_) => return 0,
        };
        if self_str.ends_with(pattern_str) {
            1
        } else {
            0
        }
    }
}

unsafe fn instring_to_str<'a>(ptr: *const u8) -> Option<&'a str> {
    unsafe {
        let bytes = instring_from_ptr(ptr)?;
        std::str::from_utf8(bytes).ok()
    }
}

unsafe fn inarray_empty() -> *const u8 {
    unsafe {
        let layout =
            std::alloc::Layout::from_size_align(16, 8).expect("valid layout for empty array");
        let ptr = std::alloc::alloc(layout);
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        *(ptr as *mut u64) = 0;
        *(ptr.add(8) as *mut u64) = 0;
        ptr
    }
}

unsafe fn inarray_from_ptrs(items: &[*const u8]) -> *const u8 {
    unsafe {
        if items.is_empty() {
            return inarray_empty();
        }
        let total = 16 + items.len() * 8;
        let layout = std::alloc::Layout::from_size_align(total, 8).expect("valid layout for array");
        let ptr = std::alloc::alloc(layout);
        if ptr.is_null() {
            std::alloc::handle_alloc_error(layout);
        }
        *(ptr as *mut u64) = items.len() as u64;
        *(ptr.add(8) as *mut u64) = items.len() as u64;
        std::ptr::copy_nonoverlapping(items.as_ptr(), ptr.add(16) as *mut *const u8, items.len());
        ptr
    }
}

/// `text.trim()` -> `String`
///
/// # Safety
/// `text_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_trim(text_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(s) = instring_to_str(text_ptr) else {
            return instring_empty();
        };
        instring_from_bytes(s.trim().as_bytes())
    }
}

/// `text.split_whitespace().collect::<Vec<String>>()` -> `[String]`
///
/// # Safety
/// `text_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_split_spaces(text_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(s) = instring_to_str(text_ptr) else {
            return inarray_empty();
        };
        let parts: Vec<*const u8> = s
            .split_whitespace()
            .map(|part| instring_from_bytes(part.as_bytes()))
            .collect();
        inarray_from_ptrs(&parts)
    }
}

/// `text.lines().collect::<Vec<String>>()` -> `[String]`
///
/// # Safety
/// `text_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_split_lines(text_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(s) = instring_to_str(text_ptr) else {
            return inarray_empty();
        };
        let parts: Vec<*const u8> = s
            .lines()
            .map(|part| instring_from_bytes(part.as_bytes()))
            .collect();
        inarray_from_ptrs(&parts)
    }
}

/// Tokenize an expression line into whitespace-separated tokens.
/// Same implementation as split_spaces for the JIT runtime.
///
/// # Safety
/// `text_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_tokenize_expr(text_ptr: *const u8) -> *const u8 {
    unsafe { in_str_split_spaces(text_ptr) }
}

/// `text.parse::<i64>()` -> `Int` (0 on failure)
///
/// # Safety
/// `text_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_to_int(text_ptr: *const u8) -> i64 {
    unsafe {
        let Some(s) = instring_to_str(text_ptr) else {
            return 0;
        };
        s.parse::<i64>().unwrap_or(0)
    }
}

/// `text.parse::<i64>().is_ok()` -> `bool`
///
/// # Safety
/// `text_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_is_int(text_ptr: *const u8) -> i64 {
    unsafe {
        let Some(s) = instring_to_str(text_ptr) else {
            return 0;
        };
        if s.trim().parse::<i64>().is_ok() {
            1
        } else {
            0
        }
    }
}

/// `text.find(pattern)` -> `Int` (-1 on failure)
///
/// # Safety
/// Both pointers must be valid, non-aliased instring pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_index_of(text_ptr: *const u8, pattern_ptr: *const u8) -> i64 {
    unsafe {
        let Some(s) = instring_to_str(text_ptr) else {
            return -1;
        };
        let Some(pattern) = instring_to_str(pattern_ptr) else {
            return -1;
        };
        s.find(pattern).map(|i| i as i64).unwrap_or(-1)
    }
}

/// `text[start..end]` -> `String`
///
/// # Safety
/// `text_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_str_slice(text_ptr: *const u8, start: i64, end: i64) -> *const u8 {
    unsafe {
        let Some(s) = instring_to_str(text_ptr) else {
            return instring_empty();
        };
        let len = s.len() as i64;
        let start = start.clamp(0, len) as usize;
        let end = end.clamp(0, len) as usize;
        let (start, end) = (start.min(end), start.max(end));
        instring_from_bytes(s[start..end].as_bytes())
    }
}

/// `n.to_string()` -> `String`
///
/// # Safety
/// No raw pointer arguments.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_int_to_string(n: i64) -> *const u8 {
    unsafe { instring_from_string(n.to_string()) }
}

/// `Path::new(a).join(b)` -> `String`
///
/// # Safety
/// Both pointers must be valid, non-aliased instring pointers or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_path_join(a_ptr: *const u8, b_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(a) = instring_to_str(a_ptr) else {
            return instring_empty();
        };
        let Some(b) = instring_to_str(b_ptr) else {
            return instring_from_bytes(a.as_bytes());
        };
        let joined = std::path::Path::new(a).join(b);
        instring_from_os_str(joined.as_os_str())
    }
}

/// `Path::new(p).parent()` -> `String`
///
/// # Safety
/// `p_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_path_dirname(p_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(p) = instring_to_str(p_ptr) else {
            return instring_empty();
        };
        match std::path::Path::new(p).parent() {
            Some(parent) => instring_from_os_str(parent.as_os_str()),
            None => instring_empty(),
        }
    }
}

/// `Path::new(p).file_name()` -> `String`
///
/// # Safety
/// `p_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_path_basename(p_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(p) = instring_to_str(p_ptr) else {
            return instring_empty();
        };
        match std::path::Path::new(p).file_name() {
            Some(name) => instring_from_os_str(name),
            None => instring_empty(),
        }
    }
}

/// `Path::new(p).extension()` -> `String`
///
/// # Safety
/// `p_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_path_extname(p_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(p) = instring_to_str(p_ptr) else {
            return instring_empty();
        };
        match std::path::Path::new(p).extension() {
            Some(ext) => instring_from_os_str(ext),
            None => instring_empty(),
        }
    }
}

/// `Path::new(p).normalize()` -> `String`
///
/// # Safety
/// `p_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_path_normalize(p_ptr: *const u8) -> *const u8 {
    unsafe {
        let Some(p) = instring_to_str(p_ptr) else {
            return instring_empty();
        };
        let normalized = std::path::Path::new(p);
        instring_from_os_str(normalized.as_os_str())
    }
}

/// `std::env::var(key).is_ok()` -> `bool`
///
/// # Safety
/// `key_ptr` must be a valid, non-aliased instring pointer or null.
#[unsafe(no_mangle)]
pub unsafe extern "C" fn in_env_has(key_ptr: *const u8) -> i64 {
    unsafe {
        let Some(key) = instring_to_str(key_ptr) else {
            return 0;
        };
        if std::env::var(key).is_ok() { 1 } else { 0 }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instring_roundtrip() {
        unsafe {
            let p = instring_from_bytes(b"hello");
            assert_eq!(p as usize % 8, 0, "instring pointer must be 8-byte aligned");
            assert_eq!(instring_from_ptr(p).unwrap(), b"hello");
        }
    }

    #[test]
    fn in_str_concat_works() {
        unsafe {
            let a = instring_from_bytes(b"hello");
            let b = instring_from_bytes(b" world");
            let c = in_str_concat(a, b);
            assert_eq!(instring_from_ptr(c).unwrap(), b"hello world");
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

    #[test]
    fn in_fs_write_and_remove_roundtrip() {
        unsafe {
            let dir = std::env::temp_dir();
            let path = dir.join(format!(
                "inauguration-stdlib-write-{}-{}",
                std::process::id(),
                std::time::SystemTime::UNIX_EPOCH
                    .elapsed()
                    .unwrap()
                    .as_nanos()
            ));
            let path_str = path.to_str().unwrap();
            let path_ptr = instring_from_bytes(path_str.as_bytes());
            let contents_ptr = instring_from_bytes(b"hello inauguration");
            let written = in_fs_write(path_ptr, contents_ptr);
            assert_eq!(written, 1);
            assert!(path.exists());
            let read = in_fs_read_to_string(path_ptr);
            assert_eq!(instring_from_ptr(read).unwrap(), b"hello inauguration");
            let removed = in_fs_remove_file(path_ptr);
            assert_eq!(removed, 1);
            assert!(!path.exists());
        }
    }

    #[test]
    fn in_fs_create_dir_roundtrip() {
        unsafe {
            let dir = std::env::temp_dir();
            let path = dir.join(format!(
                "inauguration-stdlib-dir-{}-{}",
                std::process::id(),
                std::time::SystemTime::UNIX_EPOCH
                    .elapsed()
                    .unwrap()
                    .as_nanos()
            ));
            let path_str = path.to_str().unwrap();
            let path_ptr = instring_from_bytes(path_str.as_bytes());
            let created = in_fs_create_dir(path_ptr);
            assert_eq!(created, 1);
            assert!(path.exists());
            let _ = std::fs::remove_dir(&path);
        }
    }

    #[test]
    fn in_env_set_and_remove_var_roundtrip() {
        unsafe {
            let key = format!("IN_TEST_VAR_{}", std::process::id());
            let key_ptr = instring_from_bytes(key.as_bytes());
            let value_ptr = instring_from_bytes(b"test-value");
            in_env_set_var(key_ptr, value_ptr);
            assert_eq!(std::env::var(&key).unwrap(), "test-value");
            in_env_remove_var(key_ptr);
            assert!(std::env::var(&key).is_err());
        }
    }

    #[test]
    fn in_str_contains_basic() {
        unsafe {
            let self_ptr = instring_from_bytes(b"hello world");
            let pattern_ptr = instring_from_bytes(b"world");
            assert_eq!(in_str_contains(self_ptr, pattern_ptr), 1);
            let missing_ptr = instring_from_bytes(b"nope");
            assert_eq!(in_str_contains(self_ptr, missing_ptr), 0);
        }
    }

    #[test]
    fn in_str_starts_and_ends_with_basic() {
        unsafe {
            let self_ptr = instring_from_bytes(b"hello world");
            assert_eq!(
                in_str_starts_with(self_ptr, instring_from_bytes(b"hello")),
                1
            );
            assert_eq!(
                in_str_starts_with(self_ptr, instring_from_bytes(b"world")),
                0
            );
            assert_eq!(in_str_ends_with(self_ptr, instring_from_bytes(b"world")), 1);
            assert_eq!(in_str_ends_with(self_ptr, instring_from_bytes(b"hello")), 0);
        }
    }
}
