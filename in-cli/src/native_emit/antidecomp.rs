//! Novel codegen shapes for the `harden` emit profile.
//!
//! These are intentional anti-patterns relative to conventional compilers so
//! Ghidra / Hex-Rays prologue, constant, and call-site heuristics fire less
//! cleanly. They preserve SysV ABI and observable semantics.

use crate::emit_profile::EmitProfile;
use crate::native_emit::x86_64::{self, R10, R11, RAX, RBX, REG_FP, REG_SP};
use std::cell::RefCell;

thread_local! {
    static TL_EMIT_PROFILE: RefCell<EmitProfile> = const { RefCell::new(EmitProfile::Default) };
}

pub fn set_profile(profile: EmitProfile) {
    TL_EMIT_PROFILE.with(|p| *p.borrow_mut() = profile);
}

pub fn clear_profile() {
    set_profile(EmitProfile::Default);
}

pub fn current_profile() -> EmitProfile {
    TL_EMIT_PROFILE.with(|p| *p.borrow())
}

pub fn harden_active() -> bool {
    current_profile() == EmitProfile::Harden
}

/// Unusual prologue: push rbx (callee-saved), then classic frame, then junk.
/// Caller must pair with [`harden_epilogue`].
pub fn harden_prologue() -> Vec<u8> {
    let mut code = x86_64::push_r(RBX);
    code.extend_from_slice(&x86_64::prologue());
    // Junk that preserves ABI-live state: xor r11,r11
    code.extend_from_slice(&x86_64::xor_rr(R11, R11));
    code
}

pub fn harden_epilogue() -> Vec<u8> {
    let mut code = x86_64::mov_rr(REG_SP, REG_FP);
    code.extend_from_slice(&x86_64::pop_r(REG_FP));
    code.extend_from_slice(&x86_64::pop_r(RBX));
    code.extend_from_slice(&x86_64::ret());
    code
}

/// Materialize `imm` into RAX via `(imm^mask) ^ mask` using R10 as scratch.
pub fn weird_materialize_rax(imm: i64) -> Vec<u8> {
    let mask: i64 = 0x5A5A_5A5A_5A5A_5A5A;
    let mut code = x86_64::mov_ri64(RAX, imm ^ mask);
    code.extend_from_slice(&x86_64::mov_ri64(R10, mask));
    code.extend_from_slice(&x86_64::xor_rr(RAX, R10));
    code
}

/// Insert semantic no-ops preferred before calls under harden.
pub fn junk_pad() -> Vec<u8> {
    let mut code = x86_64::xor_rr(R11, R11);
    code.extend_from_slice(&x86_64::xor_rr(R10, R10));
    code.push(0x90); // nop
    code
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn harden_prologue_starts_with_push_rbx() {
        let p = harden_prologue();
        assert_eq!(p[0], 0x53); // push rbx
    }

    #[test]
    fn profile_tls_roundtrip() {
        set_profile(EmitProfile::Harden);
        assert!(harden_active());
        clear_profile();
        assert!(!harden_active());
    }
}
