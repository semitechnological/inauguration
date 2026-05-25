//! Inauguration native runtime (`inrt`) entry contract for Mach-O executables on Apple ARM64.
//!
//! The owned native backend emits a thin `_inrt_start` stub as the process entry (`LC_MAIN`).
//! The stub calls the user entry function (for example `answer`), then exits via `SYS_exit`
//! with the integer return value in `x0` as the process exit code.

use crate::native_emit::aarch64;

pub const INRT_ENTRY_SYMBOL: &str = "_inrt_start";

/// Build the `_inrt_start` stub: `bl` to the user entry, then `SYS_exit` with `x0`.
///
/// `answer_offset` is the byte offset from the start of this stub to the entry function.
pub fn build_entry_stub(answer_offset: u32) -> Vec<u8> {
    let mut code = Vec::with_capacity(12);
    // `bl` offset is PC-relative to the `bl` instruction (stub starts at offset 0).
    code.extend_from_slice(&aarch64::bl(answer_offset as i32).to_le_bytes());
    code.extend_from_slice(&aarch64::movz64(16, 1, 0).to_le_bytes());
    code.extend_from_slice(&aarch64::svc(0x80).to_le_bytes());
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_stub_has_fixed_size() {
        assert_eq!(build_entry_stub(16).len(), 12);
    }
}
