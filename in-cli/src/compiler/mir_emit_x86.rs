//! MIR → x86_64 machine code emitter for boot image and JIT.
//!
//! Takes a [`MirModule`] and produces raw x86_64 machine code bytes
//! using the existing instruction helpers in [`crate::native_emit::x86_64`].
//!
//! ponytail: thin wrapper — uses existing instruction helpers directly.
//! Does NOT yet support the full MirOp range. Start with Mov/Add/Sub/Ret/Call
//! and extend as needed.

use super::mir::*;
use crate::native_emit::x86_64 as x64;

/// Emit a MIR module into flat x86_64 machine code suitable for boot image.
pub fn emit_boot(module: &MirModule) -> Result<(Vec<u8>, Vec<(String, u32, u32)>), String> {
    let mut code = Vec::new();
    let mut functions = Vec::new();

    for func in &module.functions {
        let func_start = code.len() as u32;
        emit_function(func, &mut code)?;
        let func_size = (code.len() as u32) - func_start;
        functions.push((func.name.clone(), func_start, func_size));
    }

    // Append rodata (string literals, constants)
    if !module.rodata.is_empty() {
        while code.len() % 8 != 0 {
            code.push(0);
        }
        code.extend_from_slice(&module.rodata);
    }

    // Apply relocations
    for reloc in &module.rodata_relocs {
        let target_off = functions
            .iter()
            .find(|(n, _, _)| n == &reloc.symbol)
            .map(|(_, s, _)| *s)
            .ok_or_else(|| format!("reloc target '{}' not found", reloc.symbol))?;
        match reloc.kind {
            RelocKind::Rel32 => {
                let disp = (target_off as i64 - reloc.offset as i64 - 5) as i32;
                code[reloc.offset as usize..reloc.offset as usize + 4]
                    .copy_from_slice(&disp.to_le_bytes());
            }
            RelocKind::Abs64 => {
                code[reloc.offset as usize..reloc.offset as usize + 8]
                    .copy_from_slice(&(target_off as u64).to_le_bytes());
            }
            RelocKind::Abs32 => {
                code[reloc.offset as usize..reloc.offset as usize + 4]
                    .copy_from_slice(&(target_off as u32).to_le_bytes());
            }
        }
    }

    Ok((code, functions))
}

fn emit_function(func: &MirFunction, code: &mut Vec<u8>) -> Result<(), String> {
    code.extend_from_slice(&x64::prologue());

    if func.frame_size > 0 {
        if func.frame_size <= 127 {
            code.extend_from_slice(&x64::sub_rsp_i8(func.frame_size as u8));
        } else {
            code.extend_from_slice(&x64::sub_rsp_i32(func.frame_size as i32));
        }
    }

    for inst in &func.instructions {
        emit_inst(inst, code)?;
    }

    // Epilogue: leave; ret
    code.push(0xc9); // leave = mov rsp, rbp; pop rbp
    code.extend_from_slice(&x64::ret());
    Ok(())
}

fn vreg(r: u32) -> Result<u8, String> {
    match r {
        0 => Ok(0),  // rax
        1 => Ok(3),  // rbx
        2 => Ok(1),  // rcx
        3 => Ok(7),  // rdi
        4 => Ok(6),  // rsi
        5 => Ok(0),  // r8 → fallback to rax
        _ => Err(format!("out of vregs: {}", r)),
    }
}

fn emit_inst(inst: &MirInst, code: &mut Vec<u8>) -> Result<(), String> {
    match inst.op {
        MirOp::Mov => match (&inst.operands[..], inst.operands.len()) {
            ([MirOperand::Reg(d), MirOperand::Imm(i)], _) => {
                code.extend_from_slice(&x64::load_i64(vreg(*d)?, *i));
            }
            ([MirOperand::Reg(d), MirOperand::Reg(s)], _) => {
                code.extend_from_slice(&x64::mov_rr(vreg(*d)?, vreg(*s)?));
            }
            ([MirOperand::Reg(d), MirOperand::Mem { base, offset }], _) => {
                // mov reg, [base + offset]
                code.extend_from_slice(&x64::mov_m_r(vreg(*base)?, *offset, vreg(*d)?));
            }
            _ => return Err("mir: unsupported mov".into()),
        },
        MirOp::Load => {
            if let [MirOperand::Reg(d), MirOperand::Reg(s)] = &inst.operands[..] {
                // mov reg, [reg] — deref pointer
                code.extend_from_slice(&x64::mov_m_r(vreg(*s)?, 0, vreg(*d)?));
            }
        }
        MirOp::Store => {
            if let [MirOperand::Mem { base, offset }, MirOperand::Reg(s)] = &inst.operands[..] {
                // mov [base + offset], reg
                code.extend_from_slice(&x64::mov_r_m(vreg(*s)?, vreg(*base)?, *offset));
            }
        }
        MirOp::Add => {
            if let [MirOperand::Reg(d), MirOperand::Reg(a), MirOperand::Reg(b)] = &inst.operands[..]
            {
                let rd = vreg(*d)?;
                code.extend_from_slice(&x64::mov_rr(rd, vreg(*a)?));
                code.extend_from_slice(&x64::add_rr(rd, vreg(*b)?));
            } else if let [MirOperand::Reg(d), MirOperand::Reg(a), MirOperand::Imm(i)] =
                &inst.operands[..]
            {
                let rd = vreg(*d)?;
                code.extend_from_slice(&x64::mov_rr(rd, vreg(*a)?));
                if *i >= i8::MIN as i64 && *i <= i8::MAX as i64 {
                    code.extend_from_slice(&x64::add_rmi8(rd, *i as u8));
                } else {
                    // Use load_i64 + add_rr
                    code.extend_from_slice(&x64::load_i64(vreg(0)?, *i));
                    code.extend_from_slice(&x64::add_rr(rd, vreg(0)?));
                }
            }
        }
        MirOp::Sub => {
            if let [MirOperand::Reg(d), MirOperand::Reg(a), MirOperand::Imm(i)] =
                &inst.operands[..]
            {
                let rd = vreg(*d)?;
                code.extend_from_slice(&x64::mov_rr(rd, vreg(*a)?));
                if *i >= i8::MIN as i64 && *i <= i8::MAX as i64 {
                    code.push(0x48);
                    code.push(0x83);
                    code.push(0xe8 | rd);
                    code.push(*i as u8 & 0xff); // sub rd, imm8
                } else {
                    code.extend_from_slice(&x64::sub_rmi32(rd, *i as i32));
                }
            }
        }
        MirOp::Mul => {
            if let [MirOperand::Reg(d), MirOperand::Reg(a), MirOperand::Reg(b)] =
                &inst.operands[..]
            {
                let rd = vreg(*d)?;
                code.extend_from_slice(&x64::mov_rr(rd, vreg(*a)?));
                code.extend_from_slice(&x64::imul_rr(rd, vreg(*b)?));
            }
        }
        MirOp::And => {
            if let [MirOperand::Reg(d), MirOperand::Reg(a), MirOperand::Reg(b)] =
                &inst.operands[..]
            {
                let rd = vreg(*d)?;
                code.extend_from_slice(&x64::mov_rr(rd, vreg(*a)?));
                // and rd, rb
                let rb = vreg(*b)?;
                if rb < 8 {
                    code.extend_from_slice(&[0x48, 0x21, 0xc0 | (rb << 3) | rd]);
                }
            }
        }
        MirOp::Or => {
            if let [MirOperand::Reg(d), MirOperand::Reg(a), MirOperand::Reg(b)] =
                &inst.operands[..]
            {
                let rd = vreg(*d)?;
                code.extend_from_slice(&x64::mov_rr(rd, vreg(*a)?));
                let rb = vreg(*b)?;
                if rb < 8 {
                    code.extend_from_slice(&[0x48, 0x09, 0xc0 | (rb << 3) | rd]);
                }
            }
        }
        MirOp::Xor => {
            if let [MirOperand::Reg(d), MirOperand::Reg(a), MirOperand::Reg(b)] =
                &inst.operands[..]
            {
                let rd = vreg(*d)?;
                code.extend_from_slice(&x64::mov_rr(rd, vreg(*a)?));
                let rb = vreg(*b)?;
                if rb < 8 {
                    code.extend_from_slice(&[0x48, 0x31, 0xc0 | (rb << 3) | rd]);
                }
            }
        }
        MirOp::Cmp => {
            if let [MirOperand::Reg(a), MirOperand::Reg(b)] = &inst.operands[..] {
                code.extend_from_slice(&x64::cmp_rr(vreg(*a)?, vreg(*b)?));
            } else if let [MirOperand::Reg(a), MirOperand::Imm(i)] = &inst.operands[..] {
                if *i >= i8::MIN as i64 && *i <= i8::MAX as i64 {
                    code.extend_from_slice(&x64::cmp_rmi8(vreg(*a)?, *i as u8));
                } else {
                    code.extend_from_slice(&x64::cmp_rmi32(vreg(*a)?, *i as i32));
                }
            }
        }
        MirOp::Push => {
            if let [MirOperand::Reg(r)] = &inst.operands[..] {
                code.extend_from_slice(&x64::push_r(vreg(*r)?));
            }
        }
        MirOp::Pop => {
            if let [MirOperand::Reg(r)] = &inst.operands[..] {
                code.extend_from_slice(&x64::pop_r(vreg(*r)?));
            }
        }
        MirOp::Ret => {} // handled by emit_function epilogue
        MirOp::Nop => {}
        MirOp::Comment(_) => {}
        _ => {
            return Err(format!("mir: op {:?} not yet supported in x86_64 emitter", inst.op));
        }
    }
    Ok(())
}
