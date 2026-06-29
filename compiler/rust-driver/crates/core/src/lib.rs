//! Inauguration Core IR — the compiler's intermediate representation.
//!
//! This is the central IR that all frontends lower into and all backends
//! consume. It replaces LLVM IR in the compiler pipeline.
//!
//! # Architecture
//!
//! ```text
//! Source (.in, Swift, C, …)
//!     │
//!     ▼  Frontend
//! IrModule  ◄── Core IR (this crate)
//!     │
//!     ▼  PassManager (hybrid-passes)
//! IrModule (optimized)
//!     │
//!     ▼  CodegenBackend (hybrid-backend)
//! ELF / Mach-O / COFF / WASM / Raw
//! ```

use serde::{Deserialize, Serialize};

// ─── Types ───────────────────────────────────────────────────────────────

/// Core IR type representation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum IrType {
    Int(i32),   // Int with bit width (e.g. Int(64))
    Float(i32), // Float with bit width (e.g. Float(32), Float(64))
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
    Array(Box<IrType>, usize), // Array type with element type and length
    Slice(Box<IrType>),        // Runtime-sized slice
    Named(String),             // Named type (struct, opaque)
    Never,                     // Bottom type (for diverging functions)
}

impl IrType {
    /// Integer type for the target machine word.
    pub const fn int(isize: i32) -> Self {
        IrType::Int(isize)
    }

    /// Size of this type in bytes, if known.
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
    // Terminators
    Branch,
    BranchCond,
    Return,
    Unreachable,

    // Memory
    Alloca,
    Load,
    Store,
    GEP,

    // Arithmetic
    Add,
    Sub,
    Mul,
    SDiv,
    UDiv,
    SRem,
    URem,

    // Bitwise
    And,
    Or,
    Xor,
    Shl,
    LShr,
    AShr,

    // Comparison
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

    // Conversion
    Trunc,
    ZExt,
    SExt,
    FPToSI,
    SIToFP,
    FPTrunc,
    FPExt,

    // Call
    Call,
    Invoke,

    // Aggregate
    ExtractValue,
    InsertValue,

    // PHI
    Phi,

    // Other
    Select,
    Fence,
    GetElementPtr,
}

/// A typed instruction in the Core IR.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IrInstruction {
    pub opcode: IrOpcode,
    pub result_type: IrType,
    pub operands: Vec<IrValue>,
    pub immediate: Option<i64>,
    /// Optional constant value (for constant-folded results).
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

    /// Allocate a new SSA value id.
    pub fn fresh_value(&mut self) -> IrValue {
        let id = self.next_value_id;
        self.next_value_id += 1;
        IrValue(id)
    }

    /// Add a basic block.
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
    /// Component specification for this module (how to compile it).
    pub component: Option<ComponentSpec>,
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

    /// Find a function by name.
    pub fn get_function(&self, name: &str) -> Option<&IrFunction> {
        self.functions.iter().find(|f| f.name == name)
    }

    /// Find a function by name (mutable).
    pub fn get_function_mut(&mut self, name: &str) -> Option<&mut IrFunction> {
        self.functions.iter_mut().find(|f| f.name == name)
    }

    /// Add a string literal and return its index.
    pub fn add_string(&mut self, s: &str) -> usize {
        let idx = self.string_literals.len();
        self.string_literals.push(s.to_string());
        idx
    }
}

// ─── Component Spec ──────────────────────────────────────────────────────

/// Kind of artifact a component produces.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ArtifactKind {
    Executable,
    SharedLibrary,
    StaticLibrary,
    ObjectFile,
    WasmModule,
    RawBinary,
}

/// Optimization level for component compilation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationLevel {
    None,
    Less,
    Default,
    Aggressive,
}

/// A component import.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentImport {
    pub name: String,
    pub interface: String,
}

/// A component export.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentExport {
    pub name: String,
    pub interface: String,
}

/// A component capability.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentCapability {
    pub name: String,
    pub capability_type: String,
    pub args: Vec<String>,
}

/// Compiler-level component specification — replaces LLVM target selection.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentSpec {
    pub name: String,
    pub target: String,
    pub artifact_kind: ArtifactKind,
    pub deterministic: bool,
    pub checkpoint: String,
    pub optimization_level: OptimizationLevel,
    pub debug_info: bool,
    pub entry_point: Option<String>,
    pub imports: Vec<ComponentImport>,
    pub exports: Vec<ComponentExport>,
    pub capabilities: Vec<ComponentCapability>,
    pub capabilities_exported: Vec<ComponentCapability>,
}

impl ComponentSpec {
    pub fn host_executable(name: &str, entry_point: Option<&str>) -> Self {
        Self {
            name: name.to_string(),
            target: Self::host_triple(),
            artifact_kind: ArtifactKind::Executable,
            deterministic: false,
            checkpoint: String::new(),
            optimization_level: OptimizationLevel::Default,
            debug_info: false,
            entry_point: entry_point.map(str::to_string),
            imports: Vec::new(),
            exports: Vec::new(),
            capabilities: Vec::new(),
            capabilities_exported: Vec::new(),
        }
    }

    pub fn host_triple() -> String {
        let os = if cfg!(target_os = "macos") {
            "apple-darwin"
        } else if cfg!(target_os = "linux") {
            "unknown-linux-gnu"
        } else if cfg!(target_os = "windows") {
            "pc-windows-msvc"
        } else {
            "unknown-none"
        };
        let arch = if cfg!(target_arch = "aarch64") {
            "aarch64"
        } else if cfg!(target_arch = "x86_64") {
            "x86_64"
        } else if cfg!(target_arch = "arm") {
            "armv7"
        } else {
            "unknown"
        };
        format!("{arch}-{os}")
    }

    pub fn object_format(&self) -> &'static str {
        if self.target.contains("apple") {
            "mach-o"
        } else if self.target.contains("-linux-") || self.target.contains("-none") {
            "elf"
        } else if self.target.contains("-windows-") {
            "coff"
        } else if self.target.starts_with("wasm32") {
            "wasm"
        } else {
            "raw"
        }
    }

    pub fn is_freestanding(&self) -> bool {
        self.target.ends_with("-none") || self.target.ends_with("-none-elf")
    }
}

// ─── Component Metadata (generic sidecar, not Space-branded) ─────────────

/// An interface method signature in a component contract.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceMethod {
    pub name: String,
    pub params: Vec<(String, IrType)>,
    pub return_type: IrType,
}

/// A declared interface that a component exports or imports.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceDecl {
    pub name: String,
    pub methods: Vec<InterfaceMethod>,
}

/// Persisted object schemas used by the component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectSchema {
    pub name: String,
    pub fields: Vec<(String, IrType)>,
}

/// Memory requirements for the component.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRequirements {
    pub stack_bytes: u64,
    pub heap_bytes: u64,
    pub static_bytes: u64,
    pub vm_object_pages: u64,
}

/// Provenance information for the build.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildProvenance {
    pub compiler_version: String,
    pub source_hash: String,
    pub build_timestamp: String,
}

/// Generic component metadata emitted alongside a compiled artifact.
///
/// This is the compiler-level metadata that can be transformed into
/// Space's SCI format or any other component-image format.
/// The metadata is intentionally generic — no Space branding.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentMetadata {
    /// Component identity (from `.in` `component` declaration or inferred).
    pub name: String,
    /// Target triple.
    pub target: String,
    /// Entry point function.
    pub entry: Option<String>,
    /// Artifact kind.
    pub artifact_kind: ArtifactKind,
    /// Declared imports (interfaces this component depends on).
    pub imports: Vec<ComponentImport>,
    /// Declared exports (interfaces this component provides).
    pub exports: Vec<ComponentExport>,
    /// Required capabilities.
    pub capabilities_required: Vec<ComponentCapability>,
    /// Exported capabilities (may create or delegate).
    pub capabilities_exported: Vec<ComponentCapability>,
    /// Declared interfaces.
    pub interfaces: Vec<InterfaceDecl>,
    /// Object schemas (persistent typed objects).
    pub object_schemas: Vec<ObjectSchema>,
    /// Memory requirements.
    pub memory: MemoryRequirements,
    /// Checkpoint policy (none, graph, state, ...).
    pub checkpoint: String,
    /// Determinism requirements.
    pub deterministic: bool,
    /// Build provenance.
    pub provenance: BuildProvenance,
}

impl ComponentMetadata {
    /// Build metadata from a component spec and IR module.
    pub fn from_spec(spec: &ComponentSpec, _module: &IrModule) -> Self {
        Self {
            name: spec.name.clone(),
            target: spec.target.clone(),
            entry: spec.entry_point.clone(),
            artifact_kind: spec.artifact_kind,
            imports: spec.imports.clone(),
            exports: spec.exports.clone(),
            capabilities_required: spec.capabilities.clone(),
            capabilities_exported: spec.capabilities_exported.clone(),
            interfaces: Vec::new(),
            object_schemas: Vec::new(),
            memory: MemoryRequirements {
                stack_bytes: 0x1000,
                heap_bytes: 0,
                static_bytes: 0,
                vm_object_pages: 0,
            },
            checkpoint: spec.checkpoint.clone(),
            deterministic: spec.deterministic,
            provenance: BuildProvenance {
                compiler_version: "0.1.0".to_string(),
                source_hash: String::new(),
                build_timestamp: String::new(),
            },
        }
    }

    /// Serialize to pretty JSON.
    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    /// Serialize to compact JSON.
    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

// ─── Diagnostics ─────────────────────────────────────────────────────────

/// Severity level for compiler diagnostics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum DiagnosticSeverity {
    Error,
    Warning,
    Note,
    Help,
}

/// A compiler diagnostic message.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Diagnostic {
    pub severity: DiagnosticSeverity,
    pub code: String,
    pub message: String,
    pub span: Option<(usize, usize)>,
    pub source_path: Option<String>,
}

impl Diagnostic {
    pub fn error(code: &str, message: &str) -> Self {
        Self {
            severity: DiagnosticSeverity::Error,
            code: code.to_string(),
            message: message.to_string(),
            span: None,
            source_path: None,
        }
    }

    pub fn warning(code: &str, message: &str) -> Self {
        Self {
            severity: DiagnosticSeverity::Warning,
            code: code.to_string(),
            message: message.to_string(),
            span: None,
            source_path: None,
        }
    }
}

// ─── Compiler Config ─────────────────────────────────────────────────────

/// Overall compiler configuration.
#[derive(Debug, Clone)]
pub struct CompilerConfig {
    /// The component spec (what to build, for which target).
    pub component: ComponentSpec,
    /// Optimization level override.
    pub optimization: OptimizationLevel,
    /// Emit debug information.
    pub debug: bool,
    /// Number of code generation threads.
    pub codegen_threads: usize,
    /// Enable all available passes.
    pub enable_all_passes: bool,
    /// Print IR after each pass.
    pub print_after_pass: bool,
}

impl CompilerConfig {
    pub fn new(component: ComponentSpec) -> Self {
        Self {
            optimization: component.optimization_level,
            debug: component.debug_info,
            codegen_threads: 1,
            enable_all_passes: true,
            print_after_pass: false,
            component,
        }
    }
}

// ─── Pipeline types (legacy compat) ──────────────────────────────────────

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ChangeEvent {
    pub path: String,
    pub module_id: String,
    pub hash: String,
    pub timestamp_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum TaskKind {
    AstRefresh,
    SwiftFrontend,
    SilAnalysis,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildTask {
    pub task_kind: TaskKind,
    pub build_id: String,
    pub deps: Vec<String>,
    pub cancel_token: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct RuntimeMetrics {
    pub p50_ms: u64,
    pub p95_ms: u64,
    pub patch_success_permille: u16,
    pub fallback_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    // ─── IrType ────────────────────────────────────────────────────────

    #[test]
    fn ir_type_int_constructor() {
        let t = IrType::int(64);
        assert_eq!(t, IrType::Int(64));
    }

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

        // Named edge cases
        assert_eq!(IrType::Named("Foo".to_string()).size_bytes(), None);
        assert_eq!(IrType::Named("".to_string()).size_bytes(), None);
    }

    #[test]
    fn ir_type_is_integer() {
        assert!(IrType::I32.is_integer());
        assert!(IrType::U64.is_integer());
        assert!(IrType::Int(32).is_integer());
        assert!(!IrType::Bool.is_integer());
        assert!(!IrType::F32.is_integer());
        assert!(!IrType::Void.is_integer());
    }

    #[test]
    fn ir_type_is_float() {
        assert!(IrType::F32.is_float());
        assert!(IrType::F64.is_float());
        assert!(IrType::Float(32).is_float());
        assert!(!IrType::I32.is_float());
    }

    // ─── IrValue ───────────────────────────────────────────────────────

    #[test]
    fn ir_value_zero_const() {
        assert!(IrValue::ZERO.is_zero());
        assert_eq!(IrValue::ZERO.0, 0);
    }

    #[test]
    fn ir_value_non_zero() {
        let v = IrValue(99);
        assert!(!v.is_zero());
    }

    // ─── IrInstruction ─────────────────────────────────────────────────

    #[test]
    fn ir_instruction_new() {
        let inst = IrInstruction::new(IrOpcode::Add, IrType::I64, vec![IrValue(1), IrValue(2)]);
        assert_eq!(inst.opcode, IrOpcode::Add);
        assert_eq!(inst.result_type, IrType::I64);
        assert_eq!(inst.operands, vec![IrValue(1), IrValue(2)]);
        assert!(inst.immediate.is_none());
        assert!(inst.constant.is_none());
    }

    #[test]
    fn ir_instruction_with_imm() {
        let inst = IrInstruction::new(IrOpcode::Sub, IrType::I32, vec![]).with_imm(10);
        assert_eq!(inst.immediate, Some(10));
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
    fn ir_function_fresh_value_increments() {
        let mut f = IrFunction::new("f", vec![], IrType::Void);
        assert_eq!(f.next_value_id, 1);
        let v1 = f.fresh_value();
        assert_eq!(v1.0, 1);
        let v2 = f.fresh_value();
        assert_eq!(v2.0, 2);
        assert_eq!(f.next_value_id, 3);
    }

    #[test]
    fn ir_function_add_block() {
        let mut f = IrFunction::new("f", vec![], IrType::Void);
        f.add_block(IrBasicBlock::new("bb0"));
        assert_eq!(f.blocks.len(), 1);
    }

    // ─── IrModule ──────────────────────────────────────────────────────

    #[test]
    fn ir_module_new() {
        let m = IrModule::new("test");
        assert_eq!(m.name, "test");
        assert!(m.functions.is_empty());
        assert!(m.component.is_none());
    }

    #[test]
    fn ir_module_get_function() {
        let mut m = IrModule::new("m");
        m.functions
            .push(IrFunction::new("main", vec![], IrType::Void));
        assert!(m.get_function("main").is_some());
        assert!(m.get_function("nope").is_none());
    }

    #[test]
    fn ir_module_add_string() {
        let mut m = IrModule::new("m");
        assert_eq!(m.add_string("hello"), 0);
        assert_eq!(m.add_string("world"), 1);
        assert_eq!(m.string_literals.len(), 2);
    }

    // ─── ComponentSpec ──────────────────────────────────────────────────

    #[test]
    fn component_spec_host_executable() {
        let spec = ComponentSpec::host_executable("app", Some("main"));
        assert_eq!(spec.name, "app");
        assert_eq!(spec.artifact_kind, ArtifactKind::Executable);
        assert_eq!(spec.entry_point, Some("main".to_string()));
        assert!(!spec.target.is_empty());
    }

    #[test]
    fn component_spec_host_triple() {
        let triple = ComponentSpec::host_triple();
        assert!(!triple.is_empty());
        // Should contain an arch and OS separator
        assert!(triple.contains('-'));
    }

    #[test]
    fn component_spec_object_format() {
        let spec_linux = ComponentSpec {
            target: "x86_64-unknown-linux-gnu".to_string(),
            ..ComponentSpec::host_executable("t", None)
        };
        assert_eq!(spec_linux.object_format(), "elf");

        let spec_mac = ComponentSpec {
            target: "aarch64-apple-darwin".to_string(),
            ..ComponentSpec::host_executable("t", None)
        };
        assert_eq!(spec_mac.object_format(), "mach-o");

        let spec_win = ComponentSpec {
            target: "x86_64-pc-windows-msvc".to_string(),
            ..ComponentSpec::host_executable("t", None)
        };
        assert_eq!(spec_win.object_format(), "coff");

        let spec_wasm = ComponentSpec {
            target: "wasm32-unknown-unknown".to_string(),
            ..ComponentSpec::host_executable("t", None)
        };
        assert_eq!(spec_wasm.object_format(), "wasm");
    }

    #[test]
    fn component_spec_is_freestanding() {
        let spec = ComponentSpec {
            target: "x86_64-unknown-none".to_string(),
            ..ComponentSpec::host_executable("t", None)
        };
        assert!(spec.is_freestanding());

        let spec2 = ComponentSpec {
            target: "aarch64-unknown-none-elf".to_string(),
            ..ComponentSpec::host_executable("t", None)
        };
        assert!(spec2.is_freestanding());

        let spec3 = ComponentSpec::host_executable("t", None);
        // host triple is not freestanding unless it ends in -none
        if !spec3.target.ends_with("-none") && !spec3.target.ends_with("-none-elf") {
            assert!(!spec3.is_freestanding());
        }
    }

    // ─── ComponentMetadata ──────────────────────────────────────────────

    #[test]
    fn component_metadata_from_spec() {
        let spec = ComponentSpec::host_executable("app", Some("main"));
        let module = IrModule::new("app");
        let meta = ComponentMetadata::from_spec(&spec, &module);
        assert_eq!(meta.name, "app");
        assert_eq!(meta.entry, Some("main".to_string()));
        assert_eq!(meta.artifact_kind, ArtifactKind::Executable);
    }

    #[test]
    fn component_metadata_json_round_trip() {
        let spec = ComponentSpec::host_executable("app", Some("main"));
        let module = IrModule::new("app");
        let meta = ComponentMetadata::from_spec(&spec, &module);
        let json = meta.to_json().unwrap();
        let meta2: ComponentMetadata = serde_json::from_str(&json).unwrap();
        assert_eq!(meta, meta2);
    }

    #[test]
    fn component_metadata_to_json_pretty() {
        let spec = ComponentSpec::host_executable("app", None);
        let module = IrModule::new("app");
        let meta = ComponentMetadata::from_spec(&spec, &module);
        let pretty = meta.to_json_pretty().unwrap();
        assert!(pretty.contains('\n'));
    }

    // ─── Diagnostic ────────────────────────────────────────────────────

    #[test]
    fn diagnostic_error() {
        let d = Diagnostic::error("E001", "something broke");
        assert_eq!(d.severity, DiagnosticSeverity::Error);
        assert_eq!(d.code, "E001");
        assert_eq!(d.message, "something broke");
        assert!(d.span.is_none());
    }

    #[test]
    fn diagnostic_warning() {
        let d = Diagnostic::warning("W001", "unused variable");
        assert_eq!(d.severity, DiagnosticSeverity::Warning);
    }

    // ─── CompilerConfig ────────────────────────────────────────────────

    #[test]
    fn compiler_config_new() {
        let spec = ComponentSpec::host_executable("app", Some("main"));
        let cfg = CompilerConfig::new(spec.clone());
        assert_eq!(cfg.optimization, OptimizationLevel::Default);
        assert!(!cfg.debug);
        assert_eq!(cfg.codegen_threads, 1);
        assert!(cfg.enable_all_passes);
        assert!(!cfg.print_after_pass);
    }

    // ─── Pipeline types ────────────────────────────────────────────────

    #[test]
    fn change_event_serde() {
        let ev = ChangeEvent {
            path: "/src/main.in".to_string(),
            module_id: "App".to_string(),
            hash: "abc123".to_string(),
            timestamp_ms: 1234567890,
        };
        let json = serde_json::to_string(&ev).unwrap();
        let ev2: ChangeEvent = serde_json::from_str(&json).unwrap();
        assert_eq!(ev, ev2);
    }

    #[test]
    fn build_task_serde() {
        let task = BuildTask {
            task_kind: TaskKind::AstRefresh,
            build_id: "b1".to_string(),
            deps: vec!["d1".to_string()],
            cancel_token: "ct".to_string(),
        };
        let json = serde_json::to_string(&task).unwrap();
        let task2: BuildTask = serde_json::from_str(&json).unwrap();
        assert_eq!(task, task2);
    }

    #[test]
    fn runtime_metrics_serde() {
        let m = RuntimeMetrics {
            p50_ms: 10,
            p95_ms: 50,
            patch_success_permille: 990,
            fallback_count: 2,
        };
        let json = serde_json::to_string(&m).unwrap();
        let m2: RuntimeMetrics = serde_json::from_str(&json).unwrap();
        assert_eq!(m, m2);
    }
}
