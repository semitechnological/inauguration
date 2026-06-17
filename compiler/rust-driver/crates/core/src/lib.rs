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
