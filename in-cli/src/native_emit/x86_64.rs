//! x86_64 instruction encoder for the owned native subset.
//!
//! Produces byte sequences for a minimal x86_64 scalar-function subset.
//! Uses Intel SDM operand encoding (ModRM, SIB, REX, immediates).

// ── Register constants ──────────────────────────────────────────

pub const RAX: u8 = 0;
pub const RCX: u8 = 1;
pub const RDX: u8 = 2;
pub const RBX: u8 = 3;
pub const RSP: u8 = 4;
pub const RBP: u8 = 5;
pub const RSI: u8 = 6;
pub const RDI: u8 = 7;
pub const REG_SP: u8 = RSP;
pub const REG_FP: u8 = RBP;
pub const REG_XZR: u8 = 0; // alias for zero idiom (xor same)
pub const REG_RET: u8 = RAX;

thread_local! {
    pub static TL_IS_32BIT: std::cell::Cell<bool> = const { std::cell::Cell::new(false) };
}

pub fn is_32bit() -> bool {
    TL_IS_32BIT.with(|cell| cell.get())
}

// ── CodeEmitter ─────────────────────────────────────────────────

#[derive(Debug, Default)]
pub struct CodeEmitter {
    pub bytes: Vec<u8>,
}

impl CodeEmitter {
    pub fn new() -> Self {
        Self { bytes: Vec::new() }
    }

    pub fn len(&self) -> u32 {
        self.bytes.len() as u32
    }

    pub fn emit_u8(&mut self, value: u8) {
        self.bytes.push(value);
    }

    pub fn emit_u16(&mut self, value: u16) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn emit_u32(&mut self, value: u32) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn emit_u64(&mut self, value: u64) {
        self.bytes.extend_from_slice(&value.to_le_bytes());
    }

    pub fn emit_bytes(&mut self, bytes: &[u8]) {
        self.bytes.extend_from_slice(bytes);
    }

    pub fn emit_insns(&mut self, insns: &[u8]) {
        self.bytes.extend_from_slice(insns);
    }

    /// Emit a 32-bit insn, return its offset.
    pub fn emit_insn(&mut self, insn: u32) -> u32 {
        let offset = self.len();
        self.emit_u32(insn);
        offset
    }

    /// Patch a previously emitted 32-bit value at `offset`.
    pub fn patch_u32(&mut self, offset: u32, value: u32) {
        let bytes = value.to_le_bytes();
        self.bytes[offset as usize..offset as usize + 4].copy_from_slice(&bytes);
    }

    /// Patch a byte at `offset`.
    pub fn patch_u8(&mut self, offset: u32, value: u8) {
        self.bytes[offset as usize] = value;
    }
}

// ── REX prefix helpers ──────────────────────────────────────────

fn rex_wb(rm: u8) -> u8 {
    0x48 | (if rm >= 8 { 1 } else { 0 })
}

fn rex_wr(reg: u8, rm: u8) -> u8 {
    0x48 | (if reg >= 8 { 1 << 2 } else { 0 }) | (if rm >= 8 { 1 } else { 0 })
}

// ── ModRM byte ──────────────────────────────────────────────────

fn modrm(mod_: u8, reg: u8, rm: u8) -> u8 {
    (mod_ << 6) | ((reg & 7) << 3) | (rm & 7)
}

// ── Instructions ────────────────────────────────────────────────

/// ret (near return)
pub fn ret() -> Vec<u8> {
    vec![0xC3]
}

/// push r/m64
pub fn push_r(reg: u8) -> Vec<u8> {
    if reg < 8 {
        vec![0x50 + reg]
    } else {
        vec![0x41, 0x50 + (reg - 8)]
    }
}

/// pop r/m64
pub fn pop_r(reg: u8) -> Vec<u8> {
    if reg < 8 {
        vec![0x58 + reg]
    } else {
        vec![0x41, 0x58 + (reg - 8)]
    }
}

/// mov r64, imm64  (REX.W + B8+rd io)
pub fn mov_ri64(reg: u8, imm: i64) -> Vec<u8> {
    let mut code = vec![rex_wb(reg), 0xB8 + (reg & 7)];
    code.extend_from_slice(&imm.to_le_bytes());
    code
}

/// mov r/m64, r64  (REX.W + 89 /r, reg=src, rm=dst)
pub fn mov_rr(dst: u8, src: u8) -> Vec<u8> {
    let mut code = vec![rex_wr(dst, src), 0x89];
    code.push(modrm(3, src & 7, dst & 7));
    code
}

/// mov r/m64, r64  (REX.W + 89 /r) — alias for mov_rr
pub fn mov_mr(dst: u8, src: u8) -> Vec<u8> {
    mov_rr(dst, src)
}

/// mov r64, [r/m64 + disp8]  (REX.W + 8B /r with ModRM)
/// Emit a SIB byte for [RSP + disp] addressing when base is RSP (4) or RBP (5).
/// Returns None if no SIB is needed.
fn sib_for_base(base: u8) -> Option<u8> {
    // RSP (4) and RBP (5) with Mod=01 or Mod=10 require a SIB byte.
    // For RSP: SIB = 0x24 (scale=0, index=4=no-index, base=4=RSP)
    if base == RSP {
        Some(0x24)
    } else if base == RBP {
        Some(0x25)
    }
    // [RBP + disp] uses SIB with index=4, base=5
    else {
        None
    }
}

/// mov [r/m64 + disp8], r64  (REX.W + 89 /r with ModRM)
/// When base is RSP (4), a SIB byte must follow ModRM.
pub fn mov_m_r(base: u8, disp: i32, reg: u8) -> Vec<u8> {
    let mut code = vec![rex_wr(reg, base)];
    code.push(0x89);
    // SIB needed only for RSP (rm=4 always requires SIB). RBP with Mod=01 works without SIB.
    let needs_sib = base == RSP;
    if disp == 0 && !needs_sib && base != RBP {
        code.push(modrm(0, reg & 7, base & 7));
    } else if disp as i8 as i32 == disp {
        code.push(modrm(1, reg & 7, base & 7));
        if needs_sib {
            code.push(sib_for_base(base).unwrap_or(0x24));
        }
        code.push(disp as u8);
    } else {
        code.push(modrm(2, reg & 7, base & 7));
        if needs_sib {
            code.push(sib_for_base(base).unwrap_or(0x24));
        }
        code.extend_from_slice(&disp.to_le_bytes());
    }
    code
}

/// mov r64, [r/m64 + disp8]  (REX.W + 8B /r with ModRM)
/// When base is RSP (4), a SIB byte must follow ModRM.
pub fn mov_r_m(reg: u8, base: u8, disp: i32) -> Vec<u8> {
    let mut code = vec![rex_wr(reg, base)];
    code.push(0x8B);
    let needs_sib = base == RSP;
    if disp == 0 && !needs_sib && base != RBP {
        code.push(modrm(0, reg & 7, base & 7));
    } else if disp as i8 as i32 == disp {
        code.push(modrm(1, reg & 7, base & 7));
        if needs_sib {
            code.push(sib_for_base(base).unwrap_or(0x24));
        }
        code.push(disp as u8);
    } else {
        code.push(modrm(2, reg & 7, base & 7));
        if needs_sib {
            code.push(sib_for_base(base).unwrap_or(0x24));
        }
        code.extend_from_slice(&disp.to_le_bytes());
    }
    code
}

/// mov [rbp - (8 + disp)], r64  — RBP-relative spill (push/pop-safe)
pub fn str64(reg: u8, disp: u16) -> Vec<u8> {
    mov_m_r(RBP, -(disp as i32 + 8), reg)
}

/// mov r64, [rbp - (8 + disp)]  — RBP-relative load (push/pop-safe)
pub fn ldr64(reg: u8, disp: u16) -> Vec<u8> {
    mov_r_m(reg, RBP, -(disp as i32 + 8))
}

/// mov rax, [abs64] — load from absolute 64-bit address (uses moffs form, rax only)
pub fn mov_rax_from_abs(addr: u64) -> Vec<u8> {
    let mut code = vec![0x48, 0xA1]; // REX.W + MOV RAX, moffs64
    code.extend_from_slice(&addr.to_le_bytes());
    code
}

/// mov [abs64], rax — store to absolute 64-bit address (uses moffs form, rax only)
pub fn mov_abs_from_rax(addr: u64) -> Vec<u8> {
    let mut code = vec![0x48, 0xA3]; // REX.W + MOV moffs64, RAX
    code.extend_from_slice(&addr.to_le_bytes());
    code
}

/// mov r64, [abs32] — load from absolute 32-bit address (any register)
pub fn mov_r_from_abs32(reg: u8, addr: u32) -> Vec<u8> {
    let mut code = vec![rex_wr(reg, 0), 0x8B];
    // ModRM: mod=00, reg=reg, r/m=100 (SIB)
    code.push(modrm(0, reg & 7, 4));
    // SIB: scale=0, index=4 (none), base=5 (disp32 = absolute)
    code.push(0x25);
    code.extend_from_slice(&addr.to_le_bytes());
    code
}

/// mov [abs32], r64 — store to absolute 32-bit address (any register)
pub fn mov_abs32_from_r(reg: u8, addr: u32) -> Vec<u8> {
    let mut code = vec![rex_wr(reg, 0), 0x89];
    code.push(modrm(0, reg & 7, 4));
    code.push(0x25);
    code.extend_from_slice(&addr.to_le_bytes());
    code
}

/// sub r/m64, imm8  (REX.W + 83 /5 ib)
pub fn sub_rmi8(dst: u8, imm: u8) -> Vec<u8> {
    let mut code = vec![rex_wb(dst), 0x83];
    code.push(modrm(3, 5, dst & 7));
    code.push(imm);
    code
}

/// sub r/m64, imm32  (REX.W + 81 /5 id)
pub fn sub_rmi32(dst: u8, imm: i32) -> Vec<u8> {
    let mut code = vec![rex_wb(dst), 0x81];
    code.push(modrm(3, 5, dst & 7));
    code.extend_from_slice(&imm.to_le_bytes());
    code
}

/// add r/m64, imm8  (REX.W + 83 /0 ib)
pub fn add_rmi8(dst: u8, imm: u8) -> Vec<u8> {
    let mut code = vec![rex_wb(dst), 0x83];
    code.push(modrm(3, 0, dst & 7));
    code.push(imm);
    code
}

/// sub rsp, imm8  — stack frame allocation
pub fn sub_rsp_i8(imm: u8) -> Vec<u8> {
    sub_rmi8(RSP, imm)
}

/// sub rsp, imm32  — stack frame allocation (large frames)
pub fn sub_rsp_i32(imm: i32) -> Vec<u8> {
    sub_rmi32(RSP, imm)
}

/// lea r64, [rsp + disp8]
pub fn lea_rsp_disp(reg: u8, disp: u8) -> Vec<u8> {
    let mut code = vec![rex_wr(reg, RSP), 0x8D];
    code.push(modrm(1, reg & 7, RSP & 7));
    code.push(disp);
    code
}

/// add r/m64, r64  (REX.W + 01 /r, reg=src, rm=dst)
pub fn add_rr(dst: u8, src: u8) -> Vec<u8> {
    let mut code = vec![rex_wr(dst, src), 0x01];
    code.push(modrm(3, src & 7, dst & 7));
    code
}

/// sub r/m64, r64  (REX.W + 29 /r, reg=src, rm=dst)
pub fn sub_rr(dst: u8, src: u8) -> Vec<u8> {
    let mut code = vec![rex_wr(dst, src), 0x29];
    code.push(modrm(3, src & 7, dst & 7));
    code
}

/// imul r64, r/m64  (REX.W + 0F AF /r, reg=dst, rm=src)
pub fn imul_rr(dst: u8, src: u8) -> Vec<u8> {
    let mut code = vec![rex_wr(dst, src), 0x0F, 0xAF];
    code.push(modrm(3, dst & 7, src & 7));
    code
}

/// xor r64, r/m64 (REX.W + 33 /r, reg=src, rm=dst) — also used for zero idiom
pub fn xor_rr(dst: u8, src: u8) -> Vec<u8> {
    let mut code = vec![rex_wr(dst, src), 0x33];
    code.push(modrm(3, src & 7, dst & 7));
    code
}

/// cmp r/m64, r64  (REX.W + 39 /r, reg=src, rm=dst)
pub fn cmp_rr(a: u8, b: u8) -> Vec<u8> {
    let mut code = vec![rex_wr(a, b), 0x39];
    code.push(modrm(3, b & 7, a & 7));
    code
}

/// cmp r/m64, imm8 (REX.W + 83 /7 ib)
pub fn cmp_rmi8(rm: u8, imm: u8) -> Vec<u8> {
    let mut code = vec![rex_wb(rm), 0x83];
    code.push(modrm(3, 7, rm & 7));
    code.push(imm);
    code
}

/// cmp r/m64, imm32 (REX.W + 81 /7 id)
pub fn cmp_rmi32(rm: u8, imm: i32) -> Vec<u8> {
    let mut code = vec![rex_wb(rm), 0x81];
    code.push(modrm(3, 7, rm & 7));
    code.extend_from_slice(&imm.to_le_bytes());
    code
}

/// jcc rel8  — conditional jump with short displacement
pub fn jcc(condition: u8, rel_offset: i8) -> Vec<u8> {
    vec![0x70 + condition, rel_offset as u8]
}

/// jcc rel32  — conditional jump with near displacement
pub fn jcc_near(condition: u8, rel_offset: i32) -> Vec<u8> {
    let mut code = vec![0x0F, 0x80 + condition];
    code.extend_from_slice(&rel_offset.to_le_bytes());
    code
}

/// je rel8
pub fn je(rel_offset: i8) -> Vec<u8> {
    jcc(0x04, rel_offset)
}

/// jne rel8
pub fn jne(rel_offset: i8) -> Vec<u8> {
    jcc(0x05, rel_offset)
}

/// jmp rel32 (near)
pub fn jmp_rel32(rel_offset: i32) -> Vec<u8> {
    let mut code = vec![0xE9];
    code.extend_from_slice(&rel_offset.to_le_bytes());
    code
}

/// jmp rel8 (short)
pub fn jmp_rel8(rel_offset: i8) -> Vec<u8> {
    vec![0xEB, rel_offset as u8]
}

/// call rel32
pub fn call_rel32(rel_offset: i32) -> Vec<u8> {
    let mut code = vec![0xE8];
    code.extend_from_slice(&rel_offset.to_le_bytes());
    code
}

/// test r/m64, r64  (REX.W + 85 /r, reg=src, rm=dst)
pub fn test_rr(a: u8, b: u8) -> Vec<u8> {
    let mut code = vec![rex_wr(a, b), 0x85];
    code.push(modrm(3, b & 7, a & 7));
    code
}

/// nop (multi-byte)
pub fn nop() -> Vec<u8> {
    vec![0x90]
}

// ── Compound sequences ──────────────────────────────────────────

/// Zero a register using `xor reg, reg`
pub fn zero_reg(reg: u8) -> Vec<u8> {
    xor_rr(reg, reg)
}

/// Load a 64-bit immediate into a register.
/// Uses `mov r64, imm64` for the general case.
pub fn load_i64(reg: u8, value: i64) -> Vec<u8> {
    if value == 0 {
        zero_reg(reg)
    } else {
        mov_ri64(reg, value)
    }
}

/// Load a 32-bit immediate (sign-extended) into a register.
pub fn load_i32(reg: u8, value: i32) -> Vec<u8> {
    if value == 0 {
        zero_reg(reg)
    } else {
        let mut code = vec![if reg >= 8 { 0x41 } else { 0x40 }, 0xB8 + (reg & 7)];
        code.extend_from_slice(&value.to_le_bytes());
        code
    }
}

/// Emit the function prologue: push rbp; mov rbp, rsp
pub fn prologue() -> Vec<u8> {
    let mut code = push_r(REG_FP);
    code.extend_from_slice(&mov_rr(REG_FP, REG_SP));
    code
}

/// Emit the function epilogue: mov rsp, rbp; pop rbp; ret
pub fn epilogue() -> Vec<u8> {
    let mut code = mov_rr(REG_SP, REG_FP);
    code.extend_from_slice(&pop_r(REG_FP));
    code.extend_from_slice(&ret());
    code
}

// ── Tests ───────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn encodes_ret() {
        assert_eq!(ret(), vec![0xC3]);
    }

    #[test]
    fn encodes_push_rbp() {
        assert_eq!(push_r(RBP), vec![0x55]);
    }

    #[test]
    fn encodes_pop_rbp() {
        assert_eq!(pop_r(RBP), vec![0x5D]);
    }

    #[test]
    fn encodes_mov_rbp_rsp() {
        assert_eq!(mov_rr(RBP, RSP), vec![0x48, 0x89, 0xE5]);
    }

    #[test]
    fn encodes_sub_rsp_imm8() {
        assert_eq!(sub_rsp_i8(0x20), vec![0x48, 0x83, 0xEC, 0x20]);
    }

    #[test]
    fn encodes_mov_rax_imm64() {
        assert_eq!(mov_ri64(RAX, 42), vec![0x48, 0xB8, 42, 0, 0, 0, 0, 0, 0, 0]);
    }

    #[test]
    fn encodes_prologue() {
        let code = prologue();
        assert_eq!(&code[..3], &[0x55, 0x48, 0x89]);
    }

    #[test]
    fn encodes_epilogue() {
        let code = epilogue();
        assert_eq!(&code[code.len() - 1..], &[0xC3]);
    }

    #[test]
    fn encodes_call_rel32() {
        let code = call_rel32(42);
        assert_eq!(code[0], 0xE8);
        assert_eq!(&code[1..5], &[42, 0, 0, 0]);
    }

    #[test]
    fn encodes_add_rax_rbx() {
        assert_eq!(add_rr(RAX, RBX), vec![0x48, 0x01, 0xD8]);
    }

    #[test]
    fn encodes_sub_rax_rbx() {
        assert_eq!(sub_rr(RAX, RBX), vec![0x48, 0x29, 0xD8]);
    }

    #[test]
    fn encodes_xor_zero() {
        let code = xor_rr(RAX, RAX);
        assert_eq!(code, vec![0x48, 0x33, 0xC0]);
    }

    #[test]
    fn encodes_cmp_rr() {
        assert_eq!(cmp_rr(RAX, RBX), vec![0x48, 0x39, 0xD8]);
    }

    #[test]
    fn encodes_je_short() {
        assert_eq!(je(0x10), vec![0x74, 0x10]);
    }

    #[test]
    fn encodes_jne_short() {
        assert_eq!(jne(0xF0_u8 as i8), vec![0x75, 0xF0]);
    }

    #[test]
    fn encodes_jmp_rel32() {
        let code = jmp_rel32(0x1000);
        assert_eq!(code[0], 0xE9);
        assert_eq!(&code[1..5], &[0x00, 0x10, 0x00, 0x00]);
    }

    #[test]
    fn encodes_jmp_rel8() {
        assert_eq!(jmp_rel8(0x20), vec![0xEB, 0x20]);
    }

    #[test]
    fn encodes_str64_and_ldr64() {
        let store = str64(RAX, 8);
        assert!(store.len() >= 3);
        let load = ldr64(RAX, 8);
        assert!(load.len() >= 3);
    }

    #[test]
    fn load_i64_zero_uses_xor() {
        let code = load_i64(RAX, 0);
        assert_eq!(code, vec![0x48, 0x33, 0xC0]);
    }

    #[test]
    fn load_i64_nonzero_uses_mov() {
        let code = load_i64(RAX, 42);
        assert_eq!(code[0], 0x48);
        assert_eq!(code[1], 0xB8);
    }

    #[test]
    fn emitter_patch_u32() {
        let mut em = CodeEmitter::new();
        let site = em.emit_insn(0);
        em.patch_u32(site, 0xDEAD_BEEF);
        assert_eq!(
            &em.bytes[site as usize..site as usize + 4],
            &[0xEF, 0xBE, 0xAD, 0xDE]
        );
    }

    #[test]
    fn prologue_epilogue_round_trip() {
        let pro = prologue();
        let epi = epilogue();
        assert!(pro.len() > 0);
        assert!(epi.len() > 0);
        // prologue first bytes: push rbp (0x55)
        assert_eq!(pro[0], 0x55);
        // epilogue last byte: ret (0xC3)
        assert_eq!(epi[epi.len() - 1], 0xC3);
    }
}
