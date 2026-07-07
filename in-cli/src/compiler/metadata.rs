//! Component metadata — the compiler's own output contract that replaces LLVM's target/lowering layer.
//!
//! Every compilation produces a `ComponentSpec` (describes *how* to build) and
//! `ComponentMetadata` (describes *what* was built) as a JSON sidecar.
//!
//! This is intentionally generic, without platform branding.

use serde::{Deserialize, Serialize};

use super::backend::BackendOutput;
use super::core::{IrModule, IrType};

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

/// A component import — declares a dependency on another component's interface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentImport {
    pub name: String,
    pub interface: String,
}

/// A component export — declares a symbol exposed to consumers of this artifact.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentExport {
    pub name: String,
    pub interface: String,
}

/// A component capability — declares a runtime privilege.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentCapability {
    pub name: String,
    pub capability_type: String,
    pub args: Vec<String>,
}

/// The compiler-level component specification — describes *how* to compile.
///
/// This replaces LLVM target selection, artifact kind, and linking configuration.
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
    /// Capabilities this component *creates or delegates* (separate from required).
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

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }

    pub fn with_host_target(mut self) -> Self {
        self.target = Self::host_triple();
        self
    }
}

// ─── Component Metadata ─────────────────────────────────────────────────

/// An interface method signature.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceMethod {
    pub name: String,
    pub params: Vec<(String, IrType)>,
    pub return_type: IrType,
}

/// A declared interface.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct InterfaceDecl {
    pub name: String,
    pub methods: Vec<InterfaceMethod>,
}

/// Code section descriptor — real size from backend.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CodeSection {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub flags: String,
}

/// Data section descriptor.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct DataSection {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub flags: String,
}

/// Field within an object schema (persistent typed object).
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectField {
    pub name: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub offset: u64,
    pub size: u64,
}

/// Persisted object schemas — describes a typed object in the object graph.
///
/// The runtime uses these schemas to allocate, persist, and validate objects.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ObjectSchema {
    pub name: String,
    pub fields: Vec<ObjectField>,
    pub size: u64,
    pub align: u64,
}

/// Memory requirements.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MemoryRequirements {
    pub stack_bytes: u64,
    pub heap_bytes: u64,
    pub static_bytes: u64,
    pub vm_object_pages: u64,
}

/// Build provenance.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BuildProvenance {
    pub compiler_version: String,
    pub source_hash: String,
    pub build_timestamp: String,
}

/// Generic component metadata emitted alongside every compiled artifact.
///
/// This is the compiler's own output contract — describes what was built,
/// what interfaces it exports/imports, what capabilities it needs, etc.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ComponentMetadata {
    pub name: String,
    pub target: String,
    pub entry: Option<String>,
    pub artifact_kind: ArtifactKind,
    /// Code sections with real sizes from the backend.
    pub code_sections: Vec<CodeSection>,
    /// Data sections.
    pub data_sections: Vec<DataSection>,
    pub imports: Vec<ComponentImport>,
    pub exports: Vec<ComponentExport>,
    /// Capabilities the component requires the loader to grant.
    pub capabilities_required: Vec<ComponentCapability>,
    /// Capabilities this component creates or delegates (separate from required).
    pub capabilities_exported: Vec<ComponentCapability>,
    pub interfaces: Vec<InterfaceDecl>,
    pub object_schemas: Vec<ObjectSchema>,
    pub memory: MemoryRequirements,
    pub checkpoint: String,
    pub deterministic: bool,
    pub provenance: BuildProvenance,
}

/// Map an [`IrType`] to its JSON schema type name.
fn ir_type_name(typ: &IrType) -> String {
    match typ {
        IrType::I8 | IrType::U8 => "i8".to_string(),
        IrType::I16 | IrType::U16 => "i16".to_string(),
        IrType::I32 | IrType::U32 | IrType::Int(32) => "i32".to_string(),
        IrType::I64 | IrType::U64 | IrType::Int(64) => "i64".to_string(),
        IrType::F32 | IrType::Float(32) => "f32".to_string(),
        IrType::F64 | IrType::Float(64) => "f64".to_string(),
        IrType::Bool => "bool".to_string(),
        IrType::Ptr(elem) => format!("*{}", ir_type_name(elem)),
        IrType::Array(elem, _) => format!("[{}]", ir_type_name(elem)),
        IrType::Slice(elem) => format!("[]{}", ir_type_name(elem)),
        IrType::Named(name) => name.clone(),
        IrType::Void => "void".to_string(),
        IrType::Never => "never".to_string(),
        _ => "i64".to_string(),
    }
}

impl ComponentMetadata {
    /// Build metadata from spec, backend output (for real code section sizes),
    /// and source (for content hash).
    pub fn build(
        spec: &ComponentSpec,
        module: &IrModule,
        output: &BackendOutput,
        source: &str,
    ) -> Self {
        // Compute source hash for provenance
        let source_hash = {
            use std::collections::hash_map::DefaultHasher;
            use std::hash::{Hash, Hasher};
            let mut hasher = DefaultHasher::new();
            source.hash(&mut hasher);
            format!("{:x}", hasher.finish())
        };

        // Extract object schemas from module struct types
        let object_schemas: Vec<ObjectSchema> = module
            .struct_types
            .iter()
            .map(|(name, fields)| {
                let mut offset = 0u64;
                let obj_fields: Vec<ObjectField> = fields
                    .iter()
                    .map(|(fn_, typ)| {
                        let size = match typ {
                            IrType::I8 | IrType::U8 => 1,
                            IrType::I16 | IrType::U16 => 2,
                            IrType::I32 | IrType::U32 | IrType::F32 => 4,
                            IrType::I64
                            | IrType::U64
                            | IrType::F64
                            | IrType::Bool
                            | IrType::Ptr(_) => 8,
                            IrType::Int(bits) => (bits / 8) as u64,
                            IrType::Float(bits) => (bits / 8) as u64,
                            IrType::Array(elem, count) => {
                                let elem_size = match elem.as_ref() {
                                    IrType::I8 | IrType::U8 => 1,
                                    IrType::I16 | IrType::U16 => 2,
                                    IrType::I32 | IrType::U32 | IrType::F32 => 4,
                                    _ => 8,
                                };
                                elem_size * (*count as u64)
                            }
                            _ => 8,
                        };
                        let off = offset;
                        offset += size;
                        ObjectField {
                            name: fn_.clone(),
                            typ: ir_type_name(typ),
                            offset: off,
                            size,
                        }
                    })
                    .collect();
                let size = offset.next_multiple_of(8);
                ObjectSchema {
                    name: name.clone(),
                    fields: obj_fields,
                    size,
                    align: 8,
                }
            })
            .collect();

        // Build code sections from backend output
        let code_sections = if output.data.is_empty() {
            vec![CodeSection {
                name: ".text".into(),
                offset: 0,
                size: output.data.len() as u64,
                flags: "rx".into(),
            }]
        } else {
            vec![CodeSection {
                name: ".text".into(),
                offset: 0,
                size: output.data.len() as u64,
                flags: "rx".into(),
            }]
        };

        Self {
            name: spec.name.clone(),
            target: spec.target.clone(),
            entry: spec.entry_point.clone(),
            artifact_kind: spec.artifact_kind,
            code_sections,
            data_sections: Vec::new(),
            imports: spec.imports.clone(),
            exports: spec.exports.clone(),
            capabilities_required: spec.capabilities.clone(),
            capabilities_exported: spec.capabilities_exported.clone(),
            interfaces: Vec::new(),
            object_schemas,
            memory: MemoryRequirements {
                stack_bytes: 0x1000,
                heap_bytes: 0,
                static_bytes: 0,
                vm_object_pages: 0,
            },
            checkpoint: spec.checkpoint.clone(),
            deterministic: spec.deterministic,
            provenance: BuildProvenance {
                compiler_version: env!("CARGO_PKG_VERSION").to_string(),
                source_hash,
                build_timestamp: String::new(),
            },
        }
    }

    pub fn to_json_pretty(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string_pretty(self)
    }

    pub fn to_json(&self) -> Result<String, serde_json::Error> {
        serde_json::to_string(self)
    }
}

#[cfg(test)]
mod tests {
    use super::super::core::{IrModule, IrType};
    use super::*;

    #[test]
    fn spec_host_executable_creates_valid_spec() {
        let spec = ComponentSpec::host_executable("test", Some("main"));
        assert_eq!(spec.name, "test");
        assert_eq!(spec.entry_point, Some("main".to_string()));
        assert_eq!(spec.artifact_kind, ArtifactKind::Executable);
        assert!(!spec.target.is_empty());
    }

    #[test]
    fn object_format_detection() {
        let spec = ComponentSpec {
            name: "t".into(),
            target: "aarch64-apple-darwin".into(),
            artifact_kind: ArtifactKind::Executable,
            deterministic: false,
            checkpoint: String::new(),
            optimization_level: OptimizationLevel::Default,
            debug_info: false,
            entry_point: None,
            imports: vec![],
            exports: vec![],
            capabilities: vec![],
            capabilities_exported: vec![],
        };
        assert_eq!(spec.object_format(), "mach-o");

        let spec2 = ComponentSpec {
            target: "x86_64-unknown-none".into(),
            ..spec
        };
        assert_eq!(spec2.object_format(), "elf");
        assert!(spec2.is_freestanding());
    }


    #[test]
    fn component_spec_json_generation() {
        let mut spec = ComponentSpec::host_executable("json-test", Some("main"));
        spec.capabilities = vec![ComponentCapability {
            name: "fs".into(),
            capability_type: "FileSystem".into(),
            args: vec!["read".into()],
        }];

        let json = spec.to_json().unwrap();
        assert!(json.contains("\"name\":\"json-test\""));
        assert!(json.contains("\"artifact_kind\":\"Executable\""));

        let pretty_json = spec.to_json_pretty().unwrap();
        assert!(pretty_json.contains("\"name\": \"json-test\""));
        assert!(pretty_json.contains("\"artifact_kind\": \"Executable\""));
    }

    #[test]
    fn component_spec_with_host_target() {
        let mut spec = ComponentSpec::host_executable("test", None);
        spec.target = "wasm32-unknown-unknown".into();
        let host_spec = spec.with_host_target();
        assert_eq!(host_spec.target, ComponentSpec::host_triple());
    }

    #[test]
    fn metadata_build_includes_code_sections_and_source_hash() {
        let spec = ComponentSpec::host_executable("my-component", Some("start"));
        let module = IrModule::new("my-component");
        let output = BackendOutput {
            data: vec![0x00; 128],
            extension: "o",
            entry_offset: Some(0),
            symbol_table: vec![],
        };
        let metadata = ComponentMetadata::build(&spec, &module, &output, "fn main() {}");
        assert_eq!(metadata.name, "my-component");
        assert!(!metadata.code_sections.is_empty());
        assert_eq!(metadata.code_sections[0].size, 128);
        assert!(!metadata.provenance.source_hash.is_empty());
        assert!(metadata.capabilities_exported.is_empty());
        let json = metadata.to_json_pretty().unwrap();
        assert!(json.contains("code_sections"));
        assert!(json.contains("source_hash"));
    }

    #[test]
    fn capabilities_separated() {
        let mut spec = ComponentSpec::host_executable("test", None);
        spec.capabilities = vec![ComponentCapability {
            name: "serial".into(),
            capability_type: "DebugConsole".into(),
            args: vec!["write".into()],
        }];
        spec.capabilities_exported = vec![ComponentCapability {
            name: "grant".into(),
            capability_type: "CapabilityTable".into(),
            args: vec!["grant".into(), "attenuate".into()],
        }];
        let module = IrModule::new("test");
        let output = BackendOutput {
            data: vec![],
            extension: "o",
            entry_offset: None,
            symbol_table: vec![],
        };
        let md = ComponentMetadata::build(&spec, &module, &output, "");
        assert_eq!(md.capabilities_required.len(), 1);
        assert_eq!(md.capabilities_exported.len(), 1);
        assert_eq!(md.capabilities_required[0].name, "serial");
        assert_eq!(md.capabilities_exported[0].name, "grant");
    }

    #[test]
    fn object_schemas_extracted_from_struct_types() {
        let mut module = IrModule::new("objects");
        module.struct_types.push((
            "User".to_string(),
            vec![
                ("id".to_string(), IrType::I64),
                ("name".to_string(), IrType::Ptr(Box::new(IrType::I8))),
                ("active".to_string(), IrType::Bool),
            ],
        ));
        module.struct_types.push((
            "Point".to_string(),
            vec![
                ("x".to_string(), IrType::F64),
                ("y".to_string(), IrType::F64),
            ],
        ));

        let spec = ComponentSpec::host_executable("objects-app", None);
        let output = BackendOutput {
            data: vec![0x00; 64],
            extension: "o",
            entry_offset: None,
            symbol_table: vec![],
        };
        let md = ComponentMetadata::build(&spec, &module, &output, "fn main() {}");

        assert_eq!(md.object_schemas.len(), 2);

        // User: id(i64@0, 8) + name(*i8@8, 8) + active(bool@16, 8) = 24
        let user = &md.object_schemas[0];
        assert_eq!(user.name, "User");
        assert_eq!(user.fields.len(), 3);
        assert_eq!(user.fields[0].name, "id");
        assert_eq!(user.fields[0].offset, 0);
        assert_eq!(user.fields[0].size, 8);
        assert_eq!(user.fields[1].name, "name");
        assert_eq!(user.fields[1].offset, 8);
        assert_eq!(user.fields[1].size, 8);
        assert_eq!(user.fields[2].name, "active");
        assert_eq!(user.fields[2].offset, 16);
        assert_eq!(user.fields[2].size, 8);
        assert_eq!(user.size, 24);

        // Point: x(f64@0, 8) + y(f64@8, 8) = 16
        let point = &md.object_schemas[1];
        assert_eq!(point.name, "Point");
        assert_eq!(point.fields.len(), 2);
        assert_eq!(point.fields[0].name, "x");
        assert_eq!(point.fields[0].offset, 0);
        assert_eq!(point.fields[1].name, "y");
        assert_eq!(point.fields[1].offset, 8);
        assert_eq!(point.size, 16);

        // Metadata JSON should include object_schemas
        let json = md.to_json_pretty().unwrap();
        assert!(json.contains("object_schemas"));
        assert!(json.contains("User"));
        assert!(json.contains("Point"));
        assert!(json.contains("\"offset\": 0"));
    }
}
