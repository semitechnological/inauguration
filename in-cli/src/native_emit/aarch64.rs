//! AArch64 instruction encoding for the owned native subset backend.

pub const REG_SP: u8 = 31;
pub const REG_XZR: u8 = 31;
pub const REG_LR: u8 = 30;
pub const REG_FP: u8 = 29;

pub fn movz64(rd: u8, imm16: u16, shift: u8) -> u32 {
    assert!(shift % 16 == 0 && shift <= 48);
    let hw = (shift / 16) as u32;
    0xD280_0000 | (hw << 21) | ((imm16 as u32) << 5) | (rd as u32)
}

pub fn movk64(rd: u8, imm16: u16, shift: u8) -> u32 {
    assert!(shift % 16 == 0 && shift <= 48);
    let hw = (shift / 16) as u32;
    0xF280_0000 | (hw << 21) | ((imm16 as u32) << 5) | (rd as u32)
}

pub fn mov_reg64(rd: u8, rm: u8) -> u32 {
    if rm == REG_SP {
        add_imm64(rd, REG_SP, 0)
    } else {
        add_reg64(rd, rm, REG_XZR)
    }
}

pub fn add_imm64(rd: u8, rn: u8, imm12: u16) -> u32 {
    0x9100_0000 | ((imm12 as u32) << 10) | ((rn as u32) << 5) | (rd as u32)
}

pub fn sub_imm64(rd: u8, rn: u8, imm12: u16) -> u32 {
    0xD100_0000 | ((imm12 as u32) << 10) | ((rn as u32) << 5) | (rd as u32)
}

pub fn add_reg64(rd: u8, rn: u8, rm: u8) -> u32 {
    0x8B00_0000 | ((rm as u32) << 16) | ((rn as u32) << 5) | (rd as u32)
}

pub fn sub_reg64(rd: u8, rn: u8, rm: u8) -> u32 {
    0xCB00_0000 | ((rm as u32) << 16) | ((rn as u32) << 5) | (rd as u32)
}

pub fn mul64(rd: u8, rn: u8, rm: u8) -> u32 {
    0x9B00_7C00 | ((rm as u32) << 16) | ((rn as u32) << 5) | (rd as u32)
}

pub fn and_reg64(rd: u8, rn: u8, rm: u8) -> u32 {
    0x8A00_0000 | ((rm as u32) << 16) | ((rn as u32) << 5) | (rd as u32)
}

pub fn orr_reg64(rd: u8, rn: u8, rm: u8) -> u32 {
    0xAA00_0000 | ((rm as u32) << 16) | ((rn as u32) << 5) | (rd as u32)
}

pub fn bl(offset_bytes: i32) -> u32 {
    let imm26 = ((offset_bytes >> 2) as u32) & 0x03FF_FFFF;
    0x9400_0000 | imm26
}

pub fn b(offset_bytes: i32) -> u32 {
    let imm26 = ((offset_bytes >> 2) as u32) & 0x03FF_FFFF;
    0x1400_0000 | imm26
}

pub fn ret() -> u32 {
    0xD65F_03C0
}

pub fn cmp_reg64(rn: u8, rm: u8) -> u32 {
    0xEB00_001F | ((rm as u32) << 16) | ((rn as u32) << 5)
}

pub fn b_cond(cond: u8, offset_bytes: i32) -> u32 {
    let imm19 = ((offset_bytes >> 2) as u32) & 0x7_FFFF;
    0x5400_0000 | (imm19 << 5) | (cond as u32)
}

pub fn stp_pre(rt: u8, rt2: u8, offset: i32) -> u32 {
    let imm7 = ((-offset / 8) as u32) & 0x7F;
    0xA980_0000 | (imm7 << 15) | ((rt2 as u32) << 10) | (REG_SP as u32) << 5 | (rt as u32)
}

pub fn ldp_post(rt: u8, rt2: u8, offset: i32) -> u32 {
    let imm7 = ((offset / 8) as u32) & 0x7F;
    0xA8C0_0000 | (imm7 << 15) | ((rt2 as u32) << 10) | (REG_SP as u32) << 5 | (rt as u32)
}

pub fn str64(rt: u8, rn: u8, offset: u32) -> u32 {
    let imm12 = (offset / 8) as u32;
    0xF900_0000 | (imm12 << 10) | ((rn as u32) << 5) | (rt as u32)
}

pub fn ldr64(rt: u8, rn: u8, offset: u32) -> u32 {
    let imm12 = (offset / 8) as u32;
    0xF940_0000 | (imm12 << 10) | ((rn as u32) << 5) | (rt as u32)
}

pub fn ldr64_reg_offset(rt: u8, rn: u8, rm: u8) -> u32 {
    0xF860_7800 | ((rm as u32) << 16) | ((rn as u32) << 5) | (rt as u32)
}

pub fn svc(imm16: u16) -> u32 {
    0xD400_0001 | ((imm16 as u32) << 5)
}

pub fn load_i64(rd: u8, value: i64) -> Vec<u32> {
    let uv = value as u64;
    let mut insns = vec![movz64(rd, (uv & 0xFFFF) as u16, 0)];
    if uv > 0xFFFF {
        insns.push(movk64(rd, ((uv >> 16) & 0xFFFF) as u16, 16));
    }
    if uv > 0xFFFF_FFFF {
        insns.push(movk64(rd, ((uv >> 32) & 0xFFFF) as u16, 32));
    }
    if uv > 0xFFFF_FFFF_FFFF {
        insns.push(movk64(rd, ((uv >> 48) & 0xFFFF) as u16, 48));
    }
    insns
}

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

    pub fn emit_u32(&mut self, insn: u32) {
        self.bytes.extend_from_slice(&insn.to_le_bytes());
    }

    pub fn emit_insn(&mut self, insn: u32) -> u32 {
        let off = self.len();
        self.emit_u32(insn);
        off
    }

    pub fn emit_insns(&mut self, insns: &[u32]) {
        for insn in insns {
            self.emit_u32(*insn);
        }
    }

    pub fn patch_u32(&mut self, offset: u32, insn: u32) {
        let start = offset as usize;
        self.bytes[start..start + 4].copy_from_slice(&insn.to_le_bytes());
    }
}

impl Default for CodeEmitter {
    fn default() -> Self {
        Self::new()
    }
}
