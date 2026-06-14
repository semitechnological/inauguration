//! Core IR — the compiler's central intermediate representation.
//!
//! All frontends lower their language AST to this IR. All backends consume it.
//! This replaces LLVM IR in the compiler pipeline.
//!
//! # IR structure
//!
//! ```text
//! IrModule
//!   ├── IrFunction
//!   │     ├── IrBasicBlock
//!   │     │     ├── instructions: Vec<IrValue>
//!   │     │     └── terminator: IrInstruction
//!   │     └── ...
//!   ├── struct_types
//!   └── ComponentSpec
//! ```

use serde::{Deserialize, Serialize};

// ─── Types ───────────────────────────────────────────────────────────────

/// Core IR type representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IrType {
    Int(i32),
    Float(i32),
    Bool,
    I8,
    I16,
    I32,
    I64,
    U8,
    U16,
    U32,
    U64,
    F32,
    F64,
    Void,
    Ptr(Box<IrType>),
    Array(Box<IrType>, usize),
    Slice(Box<IrType>),
    Named(String),
    Never,
}

impl IrType {
    pub fn size_bytes(&self) -> Option<usize> {
        match self {
            IrType::Int(8) | IrType::I8 | IrType::U8 => Some(1),
            IrType::Int(16) | IrType::I16 | IrType::U16 => Some(2),
            IrType::Int(32) | IrType::I32 | IrType::U32 | IrType::F32 => Some(4),
            IrType::Int(64) | IrType::I64 | IrType::U64 | IrType::F64 | IrType::Bool => Some(8),
            IrType::Ptr(_) => Some(8),
            IrType::Void | IrType::Never => Some(0),
            _ => None,
        }
    }

    pub fn is_integer(&self) -> bool {
        matches!(
            self,
            IrType::Int(_)
                | IrType::I8
                | IrType::I16
                | IrType::I32
                | IrType::I64
                | IrType::U8
                | IrType::U16
                | IrType::U32
                | IrType::U64
        )
    }

    pub fn is_float(&self) -> bool {
        matches!(self, IrType::Float(_) | IrType::F32 | IrType::F64)
    }
}

// ─── Values ──────────────────────────────────────────────────────────────

/// An SSA value reference.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IrValue(pub u64);

impl IrValue {
    pub const ZERO: IrValue = IrValue(0);

    pub fn is_zero(&self) -> bool {
        self.0 == 0
    }
}

/// Immediate constant values.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum IrConstant {
    Int(i64),
    UInt(u64),
    Float(f64),
    Bool(bool),
    String(String),
    NullPtr,
}

// ─── Instructions ────────────────────────────────────────────────────────

/// Core IR opcodes.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum IrOpcode {
    Branch,
    BranchCond,
    Return,
    Unreachable,
    Alloca,
    Load,
    Store,
    GEP,
    Add,
    Sub,
    Mul,
    SDiv,
    UDiv,
    SRem,
    URem,
    And,
    Or,
    Xor,
    Shl,
    LShr,
    AShr,
    Eq,
    Ne,
    Slt,
    Sle,
    Sgt,
    Sge,
    Ult,
    Ule,
    Ugt,
    Uge,
    Trunc,
    ZExt,
    SExt,
    FPToSI,
    SIToFP,
    FPTrunc,
    FPExt,
    Call,
    Invoke,
    ExtractValue,
    InsertValue,
    Phi,
    Select,
    GetElementPtr,
}

/// A typed instruction in the Core IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrInstruction {
    pub opcode: IrOpcode,
    pub result_type: IrType,
    pub operands: Vec<IrValue>,
    pub immediate: Option<i64>,
    pub constant: Option<IrConstant>,
}

impl IrInstruction {
    pub fn new(opcode: IrOpcode, result_type: IrType, operands: Vec<IrValue>) -> Self {
        Self {
            opcode,
            result_type,
            operands,
            immediate: None,
            constant: None,
        }
    }

    pub fn with_imm(mut self, imm: i64) -> Self {
        self.immediate = Some(imm);
        self
    }
}

// ─── Basic Blocks ────────────────────────────────────────────────────────

/// A basic block in the Core IR CFG.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrBasicBlock {
    pub label: String,
    pub instructions: Vec<IrValue>,
    pub terminator: Option<IrInstruction>,
}

impl IrBasicBlock {
    pub fn new(label: &str) -> Self {
        Self {
            label: label.to_string(),
            instructions: Vec::new(),
            terminator: None,
        }
    }
}

// ─── Functions ───────────────────────────────────────────────────────────

/// A function in the Core IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrFunction {
    pub name: String,
    pub params: Vec<(String, IrType)>,
    pub return_type: IrType,
    pub blocks: Vec<IrBasicBlock>,
    pub next_value_id: u64,
}

impl IrFunction {
    pub fn new(name: &str, params: Vec<(String, IrType)>, return_type: IrType) -> Self {
        Self {
            name: name.to_string(),
            params,
            return_type,
            blocks: Vec::new(),
            next_value_id: 1,
        }
    }

    pub fn fresh_value(&mut self) -> IrValue {
        let id = self.next_value_id;
        self.next_value_id += 1;
        IrValue(id)
    }

    pub fn add_block(&mut self, block: IrBasicBlock) {
        self.blocks.push(block);
    }
}

// ─── Module ──────────────────────────────────────────────────────────────

/// A complete compilation unit in the Core IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrModule {
    pub name: String,
    pub source_path: Option<String>,
    pub functions: Vec<IrFunction>,
    pub struct_types: Vec<(String, Vec<(String, IrType)>)>,
    pub string_literals: Vec<String>,
    /// Component specification for this module.
    pub component: Option<super::metadata::ComponentSpec>,
}

impl IrModule {
    pub fn new(name: &str) -> Self {
        Self {
            name: name.to_string(),
            source_path: None,
            functions: Vec::new(),
            struct_types: Vec::new(),
            string_literals: Vec::new(),
            component: None,
        }
    }

    pub fn get_function(&self, name: &str) -> Option<&IrFunction> {
        self.functions.iter().find(|f| f.name == name)
    }

    pub fn get_function_mut(&mut self, name: &str) -> Option<&mut IrFunction> {
        self.functions.iter_mut().find(|f| f.name == name)
    }

    pub fn add_string(&mut self, s: &str) -> usize {
        let idx = self.string_literals.len();
        self.string_literals.push(s.to_string());
        idx
    }
}
