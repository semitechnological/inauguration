//! MIR (Machine IR) — offset-deferred assembly inspired by Zig's codegen architecture.
//!
//! Between Core IR and native_emit, we insert a MIR stage that:
//! 1. Defines machine instructions abstractly (operands are symbolic, not encoded)
//! 2. Performs register allocation on virtual registers
//! 3. Resolves offsets and patch locations during final emit
//! 4. Produces relocatable code blocks suitable for JIT mmap or object file emission
//!
//! Zig's insight (src/codegen/*/Mir.zig): MIR is 1:1 with machine instructions but all
//! offsets are deferred. This makes MIR relocatable — you can JIT it into any memory
//! region and patch offsets at the last moment.
//!
//! ponytail: minimal implementation — virtual registers only, x86_64/aarch64 ops.
//! Add register allocation (linear scan) when performance matters.

use serde::{Deserialize, Serialize};

/// A virtual register identifier.
pub type VReg = u32;

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
    // Data movement
    Mov,    // reg, src
    Load,   // reg, mem
    Store,  // mem, reg
    Lea,    // reg, mem (load effective address)

    // Arithmetic
    Add, Sub, Mul, Div,
    And, Or, Xor,
    Shl, Shr,
    Not, Neg,

    // Float
    FAdd, FSub, FMul, FDiv,
    FMov,  // float reg, float reg/src
    FCvt,  // int ↔ float conversion

    // Control flow
    Call,   // target, [args...]
    Ret,    // [value]
    Jmp,    // label
    Jz,     // label (jump if zero)
    Jnz,    // label
    Je, Jne, Jl, Jle, Jg, Jge,
    Cmp,    // left, right (sets flags)

    // Stack
    Push, Pop,
    Alloca, // size → reg (stack allocation)

    // Function prologue/epilogue
    Prologue,
    Epilogue,

    // Pseudo (resolved during emit)
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
            .map(|f| f.instructions.len() * 16) // 16 bytes per instruction worst case
            .sum()
    }
}

/// Helper to build MIR instructions.
pub fn mir(op: MirOp, operands: Vec<MirOperand>) -> MirInst {
    MirInst {
        op,
        operands,
        offset: 0,
    }
}

pub fn vreg(id: u32) -> VReg { id }
pub fn imm(v: i64) -> MirOperand { MirOperand::Imm(v) }
pub fn label(name: &str) -> MirOperand { MirOperand::Label(name.to_string()) }
pub fn global(name: &str) -> MirOperand { MirOperand::Global(name.to_string()) }
