//! MIR (Machine IR) — unified IR with both semantic and machine levels.
//!
//! The single IR from source to machine code:
//! 1. Frontends produce MIR with Typed ops (preserving source-level types)
//! 2. MIR verification checks entry function, call targets, type consistency
//! 3. Lowering pass converts Typed → machine MirOps (allocation, schedule)
//! 4. Emit pass produces raw machine bytes (AArch64, x86_64)
//!
//! Inspired by Zig's codegen architecture: MIR is 1:1 with machine instructions
//! but all offsets are deferred, making it relocatable for JIT mmap.
//!
//! ponytail: minimal — virtual registers only, no register allocation yet.

use serde::{Deserialize, Serialize};

use crate::core_ir::Typ;

/// A virtual register identifier.
pub type VReg = u32;

// ── Machine-level operands ──────────────────────────────────────────────

/// Machine instruction operands.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MirOperand {
    /// Virtual register
    Reg(VReg),
    /// Immediate integer value
    Imm(i64),
    /// Immediate float value (bit pattern)
    ImmFloat(u64),
    /// Memory at [base + offset]
    Mem { base: VReg, offset: i32 },
    /// A label (for branches/control flow)
    Label(String),
    /// Global symbol reference (relocation)
    Global(String),
}

// ── Typed (high-level) operations ─────────────────────────────────────

/// High-level typed operations produced by direct source → MIR lowering.
/// These carry type information and are lowered to machine MirOps.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum TypedOp {
    /// Allocate a local variable of given type with given name
    Alloca(Typ, String),
    /// Load variable into register
    Load(String),
    /// Store register into variable
    Store(String),
    /// Call function
    Call(String),
    /// Integer literal
    IntLit(i64),
    /// Float literal (bit pattern)
    FloatLit(u64),
    /// String literal
    StringLit(String),
    /// Boolean literal
    BoolLit(bool),
    /// Binary operation: op, lhs_var, rhs_var
    BinOp { op: String },
    /// Unary operation: op, var
    UnaryOp { op: String },
    /// Return value (variable name)
    Return(Option<String>),
    /// Branch to a function entry
    Branch(String),
    /// No-op with source position
    Nop,
}

// ── Machine-level MIR ──────────────────────────────────────────────────

/// A single MIR instruction — 1:1 with a machine instruction.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirInst {
    pub op: MirOp,
    pub operands: Vec<MirOperand>,
    /// Byte offset from start of function (set during layout)
    pub offset: u32,
}

/// MIR opcodes — architecture-neutral, mapped to target encoding during emit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum MirOp {
    // ── Data movement ──────────────────────────────────────────
    Mov,    // reg, src
    Load,   // reg, mem
    Store,  // mem, reg
    Lea,    // reg, mem (load effective address)

    // ── Arithmetic ─────────────────────────────────────────────
    Add, Sub, Mul, Div,
    And, Or, Xor,
    Shl, Shr,
    Not, Neg,

    // ── Float ──────────────────────────────────────────────────
    FAdd, FSub, FMul, FDiv,
    FMov,  // float reg, float reg/src
    FCvt,  // int ↔ float conversion

    // ── Control flow ───────────────────────────────────────────
    Call,   // target, [args...]
    Ret,    // [value]
    Jmp,    // label
    Jz,     // label (jump if zero)
    Jnz,    // label
    Je, Jne, Jl, Jle, Jg, Jge,
    Cmp,    // left, right (sets flags)

    // ── Stack ──────────────────────────────────────────────────
    Push, Pop,
    Alloca, // size → reg (stack allocation)

    // ── Function prologue/epilogue ─────────────────────────────
    Prologue,
    Epilogue,

    // ── High-level typed ops (before machine lowering) ─────────
    Typed(TypedOp),

    // ── Pseudo (resolved during emit) ──────────────────────────
    Nop,
    Comment(String),
}

/// A compiled function in MIR form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirFunction {
    pub name: String,
    pub instructions: Vec<MirInst>,
    pub vreg_count: u32,
    pub frame_size: u32,
    /// Return type of the function
    pub return_type: Option<Typ>,
    /// Parameter types
    pub param_types: Vec<Typ>,
    /// Variable name → vreg mapping for typed ops
    pub var_map: Vec<(String, VReg)>,
}

/// A complete MIR module (one compilation unit).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirModule {
    pub functions: Vec<MirFunction>,
    pub rodata: Vec<u8>,
    pub rodata_relocs: Vec<MirRelocation>,
}

/// Relocation entry for a global symbol reference.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MirRelocation {
    pub symbol: String,
    pub offset: u32,
    pub kind: RelocKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RelocKind {
    Abs64,
    Abs32,
    Rel32,
}

impl MirModule {
    pub fn new() -> Self {
        Self {
            functions: Vec::new(),
            rodata: Vec::new(),
            rodata_relocs: Vec::new(),
        }
    }

    /// Estimate total code size (for JIT page allocation).
    pub fn estimated_code_size(&self) -> usize {
        self.functions
            .iter()
            .map(|f| f.instructions.len() * 16)
            .sum()
    }

    /// Verify the MIR module for consistency.
    /// Checks: entry function exists, call targets are resolved, types are consistent.
    pub fn verify(&self, entry: Option<&str>) -> Result<(), Vec<String>> {
        let mut errors = Vec::new();
        let func_names: Vec<&str> = self.functions.iter().map(|f| f.name.as_str()).collect();
        if let Some(entry_name) = entry {
            if !func_names.contains(&entry_name) {
                errors.push(format!("missing entry function `{entry_name}`"));
            }
        }
        for func in &self.functions {
            for inst in &func.instructions {
                if let MirOp::Typed(ref typed_op) = inst.op {
                    match typed_op {
                        TypedOp::Call(target) => {
                            if !func_names.contains(&target.as_str()) {
                                errors.push(format!("unresolved call target `{target}` in `{}`", func.name));
                            }
                        }
                        TypedOp::Branch(target) => {
                            if !func_names.contains(&target.as_str()) {
                                errors.push(format!("unresolved branch target `{target}` in `{}`", func.name));
                            }
                        }
                        TypedOp::Return(var) => {
                            if func.return_type.is_some() && var.is_none() {
                                errors.push(format!("function `{}` has return type but empty return", func.name));
                            }
                            if func.return_type.is_none() && var.is_some() {
                                errors.push(format!("function `{}` is void but returns a value", func.name));
                            }
                        }
                        _ => {}
                    }
                }
            }
        }
        if errors.is_empty() { Ok(()) } else { Err(errors) }
    }
}

// ── Helpers ────────────────────────────────────────────────────────────

pub fn mir(op: MirOp, operands: Vec<MirOperand>) -> MirInst {
    MirInst { op, operands, offset: 0 }
}

pub fn vreg(id: u32) -> VReg { id }
pub fn imm(v: i64) -> MirOperand { MirOperand::Imm(v) }
pub fn label(name: &str) -> MirOperand { MirOperand::Label(name.to_string()) }
pub fn global(name: &str) -> MirOperand { MirOperand::Global(name.to_string()) }
