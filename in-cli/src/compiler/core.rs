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

#[cfg(test)]
mod tests {
    use super::*;

    // ─── IrType ────────────────────────────────────────────────────────

    #[test]
    fn ir_type_size_bytes() {
        assert_eq!(IrType::I8.size_bytes(), Some(1));
        assert_eq!(IrType::U8.size_bytes(), Some(1));
        assert_eq!(IrType::I16.size_bytes(), Some(2));
        assert_eq!(IrType::U16.size_bytes(), Some(2));
        assert_eq!(IrType::I32.size_bytes(), Some(4));
        assert_eq!(IrType::U32.size_bytes(), Some(4));
        assert_eq!(IrType::F32.size_bytes(), Some(4));
        assert_eq!(IrType::I64.size_bytes(), Some(8));
        assert_eq!(IrType::U64.size_bytes(), Some(8));
        assert_eq!(IrType::F64.size_bytes(), Some(8));
        assert_eq!(IrType::Bool.size_bytes(), Some(8));
        assert_eq!(IrType::Ptr(Box::new(IrType::I32)).size_bytes(), Some(8));
        assert_eq!(IrType::Void.size_bytes(), Some(0));
        assert_eq!(IrType::Never.size_bytes(), Some(0));
    }

    #[test]
    fn ir_type_size_bytes_int_widths() {
        assert_eq!(IrType::Int(8).size_bytes(), Some(1));
        assert_eq!(IrType::Int(16).size_bytes(), Some(2));
        assert_eq!(IrType::Int(32).size_bytes(), Some(4));
        assert_eq!(IrType::Int(64).size_bytes(), Some(8));
    }

    #[test]
    fn ir_type_size_bytes_unknown() {
        assert_eq!(IrType::Named("Foo".to_string()).size_bytes(), None);
        assert_eq!(IrType::Array(Box::new(IrType::I32), 10).size_bytes(), None);
        assert_eq!(IrType::Slice(Box::new(IrType::I32)).size_bytes(), None);
    }

    #[test]
    fn ir_type_is_integer() {
        assert!(IrType::Int(32).is_integer());
        assert!(IrType::I8.is_integer());
        assert!(IrType::I16.is_integer());
        assert!(IrType::I32.is_integer());
        assert!(IrType::I64.is_integer());
        assert!(IrType::U8.is_integer());
        assert!(IrType::U16.is_integer());
        assert!(IrType::U32.is_integer());
        assert!(IrType::U64.is_integer());
        assert!(!IrType::F32.is_float() || !IrType::F32.is_integer());
        assert!(!IrType::Bool.is_integer());
        assert!(!IrType::Void.is_integer());
    }

    #[test]
    fn ir_type_is_float() {
        assert!(IrType::Float(32).is_float());
        assert!(IrType::Float(64).is_float());
        assert!(IrType::F32.is_float());
        assert!(IrType::F64.is_float());
        assert!(!IrType::I32.is_float());
        assert!(!IrType::Bool.is_float());
    }

    // ─── IrValue ───────────────────────────────────────────────────────

    #[test]
    fn ir_value_zero() {
        assert!(IrValue::ZERO.is_zero());
        assert_eq!(IrValue::ZERO.0, 0);
    }

    #[test]
    fn ir_value_non_zero() {
        let v = IrValue(42);
        assert!(!v.is_zero());
        assert_eq!(v.0, 42);
    }

    // ─── IrInstruction ─────────────────────────────────────────────────

    #[test]
    fn ir_instruction_new() {
        let inst = IrInstruction::new(IrOpcode::Add, IrType::I64, vec![IrValue(1), IrValue(2)]);
        assert_eq!(inst.opcode, IrOpcode::Add);
        assert_eq!(inst.result_type, IrType::I64);
        assert_eq!(inst.operands.len(), 2);
        assert!(inst.immediate.is_none());
        assert!(inst.constant.is_none());
    }

    #[test]
    fn ir_instruction_with_imm() {
        let inst = IrInstruction::new(IrOpcode::Add, IrType::I32, vec![]).with_imm(42);
        assert_eq!(inst.immediate, Some(42));
    }

    // ─── IrBasicBlock ──────────────────────────────────────────────────

    #[test]
    fn ir_basic_block_new() {
        let bb = IrBasicBlock::new("entry");
        assert_eq!(bb.label, "entry");
        assert!(bb.instructions.is_empty());
        assert!(bb.terminator.is_none());
    }

    // ─── IrFunction ────────────────────────────────────────────────────

    #[test]
    fn ir_function_new() {
        let f = IrFunction::new("main", vec![("x".to_string(), IrType::I32)], IrType::Void);
        assert_eq!(f.name, "main");
        assert_eq!(f.params.len(), 1);
        assert_eq!(f.return_type, IrType::Void);
        assert!(f.blocks.is_empty());
        assert_eq!(f.next_value_id, 1);
    }

    #[test]
    fn ir_function_fresh_value() {
        let mut f = IrFunction::new("f", vec![], IrType::Void);
        let v1 = f.fresh_value();
        let v2 = f.fresh_value();
        assert_eq!(v1.0, 1);
        assert_eq!(v2.0, 2);
        assert_eq!(f.next_value_id, 3);
    }

    #[test]
    fn ir_function_add_block() {
        let mut f = IrFunction::new("f", vec![], IrType::Void);
        f.add_block(IrBasicBlock::new("entry"));
        f.add_block(IrBasicBlock::new("exit"));
        assert_eq!(f.blocks.len(), 2);
        assert_eq!(f.blocks[0].label, "entry");
        assert_eq!(f.blocks[1].label, "exit");
    }

    // ─── IrModule ──────────────────────────────────────────────────────

    #[test]
    fn ir_module_new() {
        let m = IrModule::new("test");
        assert_eq!(m.name, "test");
        assert!(m.source_path.is_none());
        assert!(m.functions.is_empty());
        assert!(m.struct_types.is_empty());
        assert!(m.string_literals.is_empty());
        assert!(m.component.is_none());
    }

    #[test]
    fn ir_module_get_function() {
        let mut m = IrModule::new("test");
        m.functions
            .push(IrFunction::new("main", vec![], IrType::Void));
        m.functions
            .push(IrFunction::new("helper", vec![], IrType::I32));
        assert!(m.get_function("main").is_some());
        assert!(m.get_function("helper").is_some());
        assert!(m.get_function("missing").is_none());
    }

    #[test]
    fn ir_module_get_function_mut() {
        let mut m = IrModule::new("test");
        m.functions
            .push(IrFunction::new("main", vec![], IrType::Void));
        let f = m.get_function_mut("main").unwrap();
        f.add_block(IrBasicBlock::new("entry"));
        assert_eq!(m.get_function("main").unwrap().blocks.len(), 1);
    }

    #[test]
    fn ir_module_add_string() {
        let mut m = IrModule::new("test");
        let idx0 = m.add_string("hello");
        let idx1 = m.add_string("world");
        assert_eq!(idx0, 0);
        assert_eq!(idx1, 1);
        assert_eq!(m.string_literals, vec!["hello", "world"]);
    }
}
