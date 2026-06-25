//! Inauguration native runtime (`inrt`) entry contract for Mach-O executables on Apple ARM64.

use crate::native_emit::aarch64;
use std::collections::BTreeMap;

pub const INRT_ENTRY_SYMBOL: &str = "_inrt_start";

pub const INRT_BUILTINS: &[&str] = &[
    "__inrt_str_len",
    "__inrt_str_concat",
    "__inrt_str_eq",
    "__inrt_str_contains",
    "__inrt_str_substr",
    "__inrt_array_len",
    "__inrt_array_load",
    "__inrt_array_store",
    "__inrt_array_push",
];

pub fn is_inrt_builtin(name: &str) -> bool {
    INRT_BUILTINS.contains(&name)
}

pub fn inrt_builtin_param_slots(name: &str) -> Option<usize> {
    match name {
        "__inrt_str_len" | "__inrt_array_len" => Some(1),
        "__inrt_array_load" => Some(2),
        "__inrt_array_store" => Some(3),
        "__inrt_array_push" => Some(2),
        "__inrt_str_concat" | "__inrt_str_eq" | "__inrt_str_contains" => Some(2),
        "__inrt_str_substr" => Some(3),
        _ => None,
    }
}

pub fn build_entry_stub(answer_offset: u32) -> Vec<u8> {
    let mut code = Vec::with_capacity(16);
    code.extend_from_slice(&aarch64::bl(answer_offset as i32).to_le_bytes());
    code.extend_from_slice(&0xD503_201Fu32.to_le_bytes());
    code.extend_from_slice(&aarch64::movz64(16, 1, 0).to_le_bytes());
    code.extend_from_slice(&aarch64::svc(0x80).to_le_bytes());
    code
}

pub fn build_entry_stub_with_forced_exit(answer_offset: u32, exit_code: u16) -> Vec<u8> {
    let mut code = Vec::with_capacity(16);
    code.extend_from_slice(&aarch64::bl(answer_offset as i32).to_le_bytes());
    code.extend_from_slice(&aarch64::movz64(0, exit_code, 0).to_le_bytes());
    code.extend_from_slice(&aarch64::movz64(16, 1, 0).to_le_bytes());
    code.extend_from_slice(&aarch64::svc(0x80).to_le_bytes());
    code
}

fn emit_insn_blob(insns: &[u32], buf: &mut Vec<u8>) -> u32 {
    let offset = buf.len() as u32;
    for insn in insns {
        buf.extend_from_slice(&insn.to_le_bytes());
    }
    offset
}

fn align8(buf: &mut Vec<u8>) {
    while !buf.len().is_multiple_of(8) {
        buf.push(0);
    }
}

mod r {
    pub const X0: u8 = 0;
    pub const X1: u8 = 1;
    pub const X2: u8 = 2;
    pub const X3: u8 = 3;
    pub const X4: u8 = 4;
    pub const X5: u8 = 5;
    pub const X6: u8 = 6;
    pub const X7: u8 = 7;
    pub const X8: u8 = 8;
    pub const X9: u8 = 9;
    pub const X10: u8 = 10;
    pub const X16: u8 = 16;
    pub const SP: u8 = 31;
    pub const XZR: u8 = 31;
    pub const FP: u8 = 29;
    pub const LR: u8 = 30;
}
use r::*;

fn insn_lsl_imm64(rd: u8, rn: u8, shift: u8) -> u32 {
    let immr = ((-(shift as i32)) as u32) & 0x3F;
    let imms = (63u32 - shift as u32) & 0x3F;
    0xD340_0000 | (immr << 16) | (imms << 10) | ((rn as u32) << 5) | (rd as u32)
}

fn build_inrt_str_len() -> Vec<u8> {
    let mut buf = Vec::with_capacity(8);
    emit_insn_blob(&[aarch64::ldr64(X0, X0, 0), aarch64::ret()], &mut buf);
    buf
}

fn build_inrt_str_concat() -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    emit_insn_blob(
        &[
            aarch64::stp_pre(FP, LR, -32),
            aarch64::add_imm64(FP, SP, 0),
            aarch64::str64(X0, SP, 0),
            aarch64::str64(X1, SP, 8),
        ],
        &mut buf,
    );
    emit_insn_blob(
        &[aarch64::ldr64(X2, X0, 0), aarch64::ldr64(X3, X1, 0)],
        &mut buf,
    );
    emit_insn_blob(
        &[
            aarch64::add_reg64(X4, X2, X3),
            aarch64::add_imm64(X1, X4, 8),
            aarch64::str64(X4, SP, 16),
        ],
        &mut buf,
    );
    emit_insn_blob(&[aarch64::mov_reg64(X0, XZR)], &mut buf);
    emit_inline_mmap(&mut buf);
    emit_insn_blob(
        &[aarch64::ldr64(X4, SP, 16), aarch64::str64(X4, X0, 0)],
        &mut buf,
    );
    emit_insn_blob(
        &[
            aarch64::ldr64(X5, SP, 0),
            aarch64::add_imm64(X5, X5, 8),
            aarch64::add_imm64(X6, X0, 8),
            aarch64::mov_reg64(X7, XZR),
        ],
        &mut buf,
    );
    let la = buf.len() as u32;
    emit_insn_blob(
        &[
            aarch64::cmp_reg64(X7, X2),
            aarch64::b_cond(10, 5 * 4),
            aarch64::ldr64_reg_offset(X8, X5, X7),
            aarch64::str64_reg_offset(X8, X6, X7),
            aarch64::add_imm64(X7, X7, 1),
        ],
        &mut buf,
    );
    emit_insn_blob(&[aarch64::b(la as i32 - buf.len() as i32)], &mut buf);
    emit_insn_blob(
        &[
            aarch64::ldr64(X5, SP, 8),
            aarch64::add_imm64(X5, X5, 8),
            aarch64::mov_reg64(X7, X2),
            aarch64::str64(X0, SP, 16),
        ],
        &mut buf,
    );
    let lb = buf.len() as u32;
    emit_insn_blob(
        &[
            aarch64::cmp_reg64(X7, X4),
            aarch64::b_cond(10, 5 * 4),
            aarch64::ldr64_reg_offset(X8, X5, X7),
            aarch64::ldr64(X3, SP, 16),
            aarch64::add_imm64(X3, X3, 8),
            aarch64::str64_reg_offset(X8, X3, X7),
            aarch64::add_imm64(X7, X7, 1),
        ],
        &mut buf,
    );
    emit_insn_blob(&[aarch64::b(lb as i32 - buf.len() as i32)], &mut buf);
    emit_insn_blob(
        &[
            aarch64::ldr64(X0, SP, 0),
            aarch64::ldp_post(FP, LR, 32),
            aarch64::ret(),
        ],
        &mut buf,
    );
    buf
}

fn build_inrt_str_eq() -> Vec<u8> {
    let mut buf = Vec::with_capacity(128);
    // prologue: save fp, lr
    emit_insn_blob(
        &[
            aarch64::stp_pre(FP, LR, -16),
            aarch64::add_imm64(FP, SP, 0),
        ],
        &mut buf,
    );
    // load lengths
    emit_insn_blob(
        &[
            aarch64::ldr64(X2, X0, 0),
            aarch64::ldr64(X3, X1, 0),
            aarch64::cmp_reg64(X2, X3),
        ],
        &mut buf,
    );
    // forward: b.ne -> not_equal
    let ne_len = buf.len() as u32;
    emit_insn_blob(&[aarch64::b_cond(1, 0)], &mut buf);
    // setup loop: p1 = str1+8, p2 = str2+8, remaining = len (same for both)
    emit_insn_blob(
        &[
            aarch64::add_imm64(X4, X0, 8),
            aarch64::add_imm64(X5, X1, 8),
            aarch64::mov_reg64(X6, X2),
        ],
        &mut buf,
    );
    // loop: check remaining == 0
    let l_loop = buf.len() as u32;
    emit_insn_blob(&[aarch64::cmp_reg64(X6, XZR)], &mut buf);
    let eq_rem = buf.len() as u32;
    emit_insn_blob(&[aarch64::b_cond(0, 0)], &mut buf);
    // compare byte
    emit_insn_blob(
        &[
            aarch64::ldrb(X8, X4, 0),
            aarch64::ldrb(X9, X5, 0),
            aarch64::cmp_reg64(X8, X9),
        ],
        &mut buf,
    );
    let ne_byte = buf.len() as u32;
    emit_insn_blob(&[aarch64::b_cond(1, 0)], &mut buf);
    // advance ptrs, decrement remaining, loop
    emit_insn_blob(
        &[
            aarch64::add_imm64(X4, X4, 1),
            aarch64::add_imm64(X5, X5, 1),
            aarch64::sub_imm64(X6, X6, 1),
            aarch64::b(l_loop as i32 - buf.len() as i32),
        ],
        &mut buf,
    );
    // equal: return 1
    let l_equal = buf.len() as u32;
    emit_insn_blob(&[aarch64::movz64(X0, 1, 0)], &mut buf);
    let b_done = buf.len() as u32;
    emit_insn_blob(&[aarch64::b(0)], &mut buf);
    // not_equal: return 0
    let l_ne = buf.len() as u32;
    emit_insn_blob(&[aarch64::mov_reg64(X0, XZR)], &mut buf);
    // done: epilogue
    let l_done = buf.len() as u32;
    emit_insn_blob(&[aarch64::ldp_post(FP, LR, 16), aarch64::ret()], &mut buf);
    // patch forward branches
    buf[ne_len as usize..ne_len as usize + 4]
        .copy_from_slice(&aarch64::b_cond(1, l_ne as i32 - ne_len as i32).to_le_bytes());
    buf[eq_rem as usize..eq_rem as usize + 4]
        .copy_from_slice(&aarch64::b_cond(0, l_equal as i32 - eq_rem as i32).to_le_bytes());
    buf[ne_byte as usize..ne_byte as usize + 4]
        .copy_from_slice(&aarch64::b_cond(1, l_ne as i32 - ne_byte as i32).to_le_bytes());
    buf[b_done as usize..b_done as usize + 4]
        .copy_from_slice(&aarch64::b(l_done as i32 - b_done as i32).to_le_bytes());
    buf
}

fn build_inrt_str_contains() -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    // prologue: save fp, lr, callee-saved regs
    emit_insn_blob(
        &[
            aarch64::stp_pre(FP, LR, -64),
            aarch64::add_imm64(FP, SP, 0),
            aarch64::str64(X0, SP, 0),
            aarch64::str64(X1, SP, 8),
        ],
        &mut buf,
    );
    // load lengths
    emit_insn_blob(
        &[
            aarch64::ldr64(X2, X0, 0),
            aarch64::ldr64(X3, X1, 0),
            aarch64::str64(X2, SP, 16),
            aarch64::str64(X3, SP, 24),
        ],
        &mut buf,
    );
    // if n_len == 0: return 1 (empty needle matches)
    emit_insn_blob(&[aarch64::cmp_reg64(X3, XZR)], &mut buf);
    let eq_zero = buf.len() as u32;
    emit_insn_blob(&[aarch64::b_cond(0, 0)], &mut buf);
    // if n_len > h_len: return 0
    emit_insn_blob(&[aarch64::cmp_reg64(X3, X2)], &mut buf);
    let hi_nlen = buf.len() as u32;
    emit_insn_blob(&[aarch64::b_cond(8, 0)], &mut buf);
    // max_start = h_len - n_len + 1
    emit_insn_blob(
        &[
            aarch64::sub_reg64(X4, X2, X3),
            aarch64::add_imm64(X4, X4, 1),
            aarch64::str64(X4, SP, 32),
            aarch64::mov_reg64(X5, XZR),
            aarch64::str64(X5, SP, 40),
        ],
        &mut buf,
    );
    // outer_loop: check i >= max_start
    let l_outer = buf.len() as u32;
    emit_insn_blob(
        &[
            aarch64::ldr64(X4, SP, 32),
            aarch64::ldr64(X5, SP, 40),
            aarch64::cmp_reg64(X5, X4),
        ],
        &mut buf,
    );
    let hs_outer = buf.len() as u32;
    emit_insn_blob(&[aarch64::b_cond(2, 0)], &mut buf);
    // setup inner loop: haystack[i..i+n_len] vs needle[0..n_len]
    emit_insn_blob(
        &[
            aarch64::ldr64(X0, SP, 0),
            aarch64::ldr64(X1, SP, 8),
            aarch64::ldr64(X3, SP, 24),
            aarch64::add_imm64(X6, X0, 8),
            aarch64::add_imm64(X7, X1, 8),
            aarch64::add_reg64(X6, X6, X5),
            aarch64::mov_reg64(X8, XZR),
        ],
        &mut buf,
    );
    // inner_loop: check j >= n_len
    let l_inner = buf.len() as u32;
    emit_insn_blob(&[aarch64::cmp_reg64(X8, X3)], &mut buf);
    let hs_inner = buf.len() as u32;
    emit_insn_blob(&[aarch64::b_cond(2, 0)], &mut buf);
    // compare byte
    emit_insn_blob(
        &[
            aarch64::ldrb(X9, X6, 0),
            aarch64::ldrb(X10, X7, 0),
            aarch64::cmp_reg64(X9, X10),
        ],
        &mut buf,
    );
    let ne_byte = buf.len() as u32;
    emit_insn_blob(&[aarch64::b_cond(1, 0)], &mut buf);
    // advance inner loop
    emit_insn_blob(
        &[
            aarch64::add_imm64(X6, X6, 1),
            aarch64::add_imm64(X7, X7, 1),
            aarch64::add_imm64(X8, X8, 1),
            aarch64::b(l_inner as i32 - buf.len() as i32),
        ],
        &mut buf,
    );
    // next: advance outer loop
    let l_next = buf.len() as u32;
    emit_insn_blob(
        &[
            aarch64::ldr64(X5, SP, 40),
            aarch64::add_imm64(X5, X5, 1),
            aarch64::str64(X5, SP, 40),
            aarch64::b(l_outer as i32 - buf.len() as i32),
        ],
        &mut buf,
    );
    // found: return 1
    let l_found = buf.len() as u32;
    emit_insn_blob(&[aarch64::movz64(X0, 1, 0)], &mut buf);
    let b_done = buf.len() as u32;
    emit_insn_blob(&[aarch64::b(0)], &mut buf);
    // not_found: return 0
    let l_nf = buf.len() as u32;
    emit_insn_blob(&[aarch64::mov_reg64(X0, XZR)], &mut buf);
    // done: epilogue
    let l_done = buf.len() as u32;
    emit_insn_blob(&[aarch64::ldp_post(FP, LR, 64), aarch64::ret()], &mut buf);
    // patch forward branches
    buf[eq_zero as usize..eq_zero as usize + 4]
        .copy_from_slice(&aarch64::b_cond(0, l_found as i32 - eq_zero as i32).to_le_bytes());
    buf[hi_nlen as usize..hi_nlen as usize + 4]
        .copy_from_slice(&aarch64::b_cond(8, l_nf as i32 - hi_nlen as i32).to_le_bytes());
    buf[hs_outer as usize..hs_outer as usize + 4]
        .copy_from_slice(&aarch64::b_cond(2, l_nf as i32 - hs_outer as i32).to_le_bytes());
    buf[hs_inner as usize..hs_inner as usize + 4]
        .copy_from_slice(&aarch64::b_cond(2, l_found as i32 - hs_inner as i32).to_le_bytes());
    buf[ne_byte as usize..ne_byte as usize + 4]
        .copy_from_slice(&aarch64::b_cond(1, l_next as i32 - ne_byte as i32).to_le_bytes());
    buf[b_done as usize..b_done as usize + 4]
        .copy_from_slice(&aarch64::b(l_done as i32 - b_done as i32).to_le_bytes());
    buf
}

fn build_inrt_str_substr() -> Vec<u8> {
    let mut buf = Vec::with_capacity(256);
    emit_insn_blob(
        &[
            aarch64::stp_pre(FP, LR, -48),
            aarch64::add_imm64(FP, SP, 0),
            aarch64::str64(X0, SP, 0),
            aarch64::str64(X1, SP, 8),
            aarch64::str64(X2, SP, 16),
        ],
        &mut buf,
    );
    emit_insn_blob(
        &[
            aarch64::cmp_reg64(X1, XZR),
            aarch64::b_cond(11, 40),
            aarch64::ldr64(X3, X0, 0),
            aarch64::add_reg64(X4, X1, X2),
            aarch64::cmp_reg64(X4, X3),
            aarch64::b_cond(12, 36),
        ],
        &mut buf,
    );
    emit_insn_blob(
        &[aarch64::add_imm64(X1, X2, 8), aarch64::mov_reg64(X0, XZR)],
        &mut buf,
    );
    emit_inline_mmap(&mut buf);
    emit_insn_blob(
        &[
            aarch64::ldr64(X2, SP, 16),
            aarch64::str64(X2, X0, 0),
            aarch64::str64(X0, SP, 24),
        ],
        &mut buf,
    );
    emit_insn_blob(
        &[
            aarch64::ldr64(X3, SP, 0),
            aarch64::add_imm64(X3, X3, 8),
            aarch64::ldr64(X4, SP, 8),
            aarch64::add_imm64(X5, X0, 8),
            aarch64::mov_reg64(X6, XZR),
        ],
        &mut buf,
    );
    let cl = buf.len() as u32;
    emit_insn_blob(
        &[
            aarch64::ldr64(X7, SP, 16),
            aarch64::cmp_reg64(X6, X7),
            aarch64::b_cond(10, 5 * 4),
            aarch64::add_reg64(X9, X4, X6),
            aarch64::ldr64_reg_offset(X8, X3, X9),
            aarch64::str64_reg_offset(X8, X5, X6),
            aarch64::add_imm64(X6, X6, 1),
        ],
        &mut buf,
    );
    emit_insn_blob(&[aarch64::b(cl as i32 - buf.len() as i32)], &mut buf);
    emit_insn_blob(
        &[
            aarch64::ldr64(X0, SP, 24),
            aarch64::ldp_post(FP, LR, 48),
            aarch64::ret(),
        ],
        &mut buf,
    );
    emit_insn_blob(
        &[
            aarch64::mov_reg64(X0, XZR),
            aarch64::ldp_post(FP, LR, 48),
            aarch64::ret(),
        ],
        &mut buf,
    );
    buf
}

fn build_inrt_array_len() -> Vec<u8> {
    let mut buf = Vec::with_capacity(8);
    emit_insn_blob(&[aarch64::ldr64(X0, X0, 0), aarch64::ret()], &mut buf);
    buf
}

fn build_inrt_array_load() -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);
    emit_insn_blob(
        &[
            aarch64::ldr64(X2, X0, 0),
            aarch64::cmp_reg64(X1, XZR),
            aarch64::b_cond(11, 16),
            aarch64::cmp_reg64(X1, X2),
            aarch64::b_cond(10, 12),
            aarch64::add_imm64(X0, X0, 16),
            aarch64::ldr64_reg_offset(X0, X0, X1),
            aarch64::ret(),
            aarch64::mov_reg64(X0, XZR),
            aarch64::ret(),
        ],
        &mut buf,
    );
    buf
}

fn build_inrt_array_store() -> Vec<u8> {
    let mut buf = Vec::with_capacity(64);
    emit_insn_blob(
        &[
            aarch64::ldr64(X3, X0, 0),
            aarch64::cmp_reg64(X1, XZR),
            aarch64::b_cond(11, 16),
            aarch64::cmp_reg64(X1, X3),
            aarch64::b_cond(10, 12),
            aarch64::add_imm64(X0, X0, 16),
            aarch64::str64_reg_offset(X2, X0, X1),
            aarch64::ret(),
            aarch64::mov_reg64(X0, XZR),
            aarch64::ret(),
        ],
        &mut buf,
    );
    buf
}

fn build_inrt_array_push() -> Vec<u8> {
    let mut buf = Vec::with_capacity(384);
    emit_insn_blob(
        &[
            aarch64::stp_pre(FP, LR, -64),
            aarch64::add_imm64(FP, SP, 0),
            aarch64::str64(X0, SP, 0),
            aarch64::str64(X1, SP, 8),
        ],
        &mut buf,
    );
    emit_insn_blob(
        &[
            aarch64::ldr64(X2, X0, 0),
            aarch64::ldr64(X3, X0, 8),
            aarch64::str64(X2, SP, 16),
            aarch64::str64(X3, SP, 24),
        ],
        &mut buf,
    );
    emit_insn_blob(
        &[aarch64::cmp_reg64(X2, X3), aarch64::b_cond(11, 400)],
        &mut buf,
    );
    let gs = buf.len() as u32;
    emit_insn_blob(
        &[
            aarch64::cmp_reg64(X3, XZR),
            aarch64::b_cond(1, 8),
            aarch64::movz64(X3, 4, 0),
            aarch64::b(4),
        ],
        &mut buf,
    );
    emit_insn_blob(&[insn_lsl_imm64(X3, X3, 1)], &mut buf);
    emit_insn_blob(
        &[
            aarch64::add_imm64(X5, X3, 2),
            insn_lsl_imm64(X1, X5, 3),
            aarch64::str64(X3, SP, 24),
        ],
        &mut buf,
    );
    emit_insn_blob(&[aarch64::mov_reg64(X0, XZR)], &mut buf);
    emit_inline_mmap(&mut buf);
    emit_insn_blob(
        &[
            aarch64::ldr64(X2, SP, 16),
            aarch64::ldr64(X3, SP, 24),
            aarch64::str64(X2, X0, 0),
            aarch64::str64(X3, X0, 8),
            aarch64::str64(X0, SP, 32),
        ],
        &mut buf,
    );
    emit_insn_blob(
        &[
            aarch64::ldr64(X4, SP, 0),
            aarch64::add_imm64(X5, X4, 16),
            aarch64::add_imm64(X6, X0, 16),
            aarch64::mov_reg64(X7, XZR),
        ],
        &mut buf,
    );
    let cl = buf.len() as u32;
    emit_insn_blob(
        &[
            aarch64::ldr64(X2, SP, 16),
            aarch64::cmp_reg64(X7, X2),
            aarch64::b_cond(10, 5 * 4),
            aarch64::ldr64_reg_offset(X8, X5, X7),
            aarch64::str64_reg_offset(X8, X6, X7),
            aarch64::add_imm64(X7, X7, 1),
        ],
        &mut buf,
    );
    emit_insn_blob(&[aarch64::b(cl as i32 - buf.len() as i32)], &mut buf);
    emit_insn_blob(
        &[aarch64::ldr64(X0, SP, 32), aarch64::str64(X0, SP, 0)],
        &mut buf,
    );
    let ng = buf.len() as u32;
    buf[(gs - 4) as usize..(gs - 4 + 4) as usize]
        .copy_from_slice(&aarch64::b_cond(11, ng as i32 - (gs - 4) as i32).to_le_bytes());
    emit_insn_blob(
        &[
            aarch64::ldr64(X0, SP, 0),
            aarch64::ldr64(X2, SP, 16),
            aarch64::ldr64(X1, SP, 8),
            aarch64::add_imm64(X3, X0, 16),
            aarch64::str64_reg_offset(X1, X3, X2),
            aarch64::add_imm64(X2, X2, 1),
            aarch64::str64(X2, X0, 0),
            aarch64::mov_reg64(X0, X2),
            aarch64::ldp_post(FP, LR, 64),
            aarch64::ret(),
        ],
        &mut buf,
    );
    buf
}

fn emit_inline_mmap(buf: &mut Vec<u8>) {
    emit_insn_blob(
        &[
            aarch64::movz64(X2, 3, 0),
            aarch64::movz64(X3, 0x1001, 0),
            aarch64::movz64(X4, 1, 0),
            aarch64::sub_reg64(X4, XZR, X4),
            aarch64::mov_reg64(X5, XZR),
            aarch64::movz64(X16, 0xC5, 0),
            aarch64::movk64(X16, 2, 16),
            aarch64::svc(0x80),
        ],
        buf,
    );
}

pub fn build_runtime_blob() -> (Vec<u8>, BTreeMap<String, u32>) {
    let mut blob = Vec::with_capacity(2048);
    let mut offsets = BTreeMap::new();
    macro_rules! add_fn {
        ($name:expr, $builder:expr) => {{
            align8(&mut blob);
            let off = $builder;
            offsets.insert($name.to_string(), blob.len() as u32);
            blob.extend_from_slice(&off);
        }};
    }
    add_fn!("__inrt_str_len", build_inrt_str_len());
    add_fn!("__inrt_str_concat", build_inrt_str_concat());
    add_fn!("__inrt_str_eq", build_inrt_str_eq());
    add_fn!("__inrt_str_contains", build_inrt_str_contains());
    add_fn!("__inrt_str_substr", build_inrt_str_substr());
    add_fn!("__inrt_array_len", build_inrt_array_len());
    add_fn!("__inrt_array_load", build_inrt_array_load());
    add_fn!("__inrt_array_store", build_inrt_array_store());
    add_fn!("__inrt_array_push", build_inrt_array_push());
    (blob, offsets)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn entry_stub_has_fixed_size() {
        assert_eq!(build_entry_stub(16).len(), 16);
        assert_eq!(build_entry_stub_with_forced_exit(16, 0).len(), 16);
    }
    #[test]
    fn runtime_blob_has_all_builtins() {
        let (_, offsets) = build_runtime_blob();
        for name in INRT_BUILTINS {
            assert!(offsets.contains_key(*name), "missing {name}");
        }
    }
    #[test]
    fn str_len_is_small() {
        assert!(build_inrt_str_len().len() <= 16);
    }
    #[test]
    fn array_load_has_bounds_check() {
        let code = build_inrt_array_load();
        let words: Vec<u32> = code
            .chunks_exact(4)
            .map(|b| u32::from_le_bytes(b.try_into().unwrap()))
            .collect();
        assert!(words.contains(&aarch64::cmp_reg64(1, aarch64::REG_XZR)));
    }
    #[test]
    fn all_builtins_nonempty_aligned() {
        for name in INRT_BUILTINS {
            let f = match *name {
                "__inrt_str_len" => build_inrt_str_len(),
                "__inrt_str_concat" => build_inrt_str_concat(),
                "__inrt_str_eq" => build_inrt_str_eq(),
                "__inrt_str_contains" => build_inrt_str_contains(),
                "__inrt_str_substr" => build_inrt_str_substr(),
                "__inrt_array_len" => build_inrt_array_len(),
                "__inrt_array_load" => build_inrt_array_load(),
                "__inrt_array_store" => build_inrt_array_store(),
                "__inrt_array_push" => build_inrt_array_push(),
                _ => continue,
            };
            assert!(!f.is_empty() && f.len() % 4 == 0, "{name} invalid");
        }
    }
    #[test]
    fn is_inrt_builtin_matches() {
        assert!(is_inrt_builtin("__inrt_str_len"));
        assert!(!is_inrt_builtin("malloc"));
    }
    #[test]
    fn param_slots_match() {
        assert_eq!(inrt_builtin_param_slots("__inrt_str_len"), Some(1));
        assert_eq!(inrt_builtin_param_slots("__inrt_array_load"), Some(2));
        assert_eq!(inrt_builtin_param_slots("nope"), None);
    }
}
