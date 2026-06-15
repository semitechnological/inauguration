use crate::core_ir::UnifiedModule;
use serde::{Deserialize, Serialize};

pub const IN_ABI_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundaryRepr {
    #[serde(rename = "c")]
    C,
    #[serde(rename = "transparent")]
    Transparent,
    #[serde(rename = "packed")]
    Packed,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundaryTransfer {
    #[serde(rename = "copy")]
    Copy,
    #[serde(rename = "borrow")]
    Borrow,
    #[serde(rename = "owned")]
    Owned,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BoundaryOwnership {
    #[serde(rename = "returns-owned-handle")]
    ReturnsOwnedHandle,
    #[serde(rename = "borrowed")]
    Borrowed,
    #[serde(rename = "owned-buffer")]
    OwnedBuffer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryField {
    pub name: String,
    pub offset: u64,
    #[serde(rename = "type")]
    pub typ: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub transfer: Option<BoundaryTransfer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryLayout {
    pub name: String,
    pub kind: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repr: Option<BoundaryRepr>,
    pub size: u64,
    pub align: u64,
    pub stride: u64,
    pub fields: Vec<BoundaryField>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundarySymbol {
    pub name: String,
    pub signature_hash: String,
    pub ownership: BoundaryOwnership,
    #[serde(default = "default_calling_convention")]
    pub calling_convention: String,
}

fn default_calling_convention() -> String {
    "c".to_string()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryAllocator {
    pub id: u64,
    pub kind: String,
    pub free_with: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BoundaryModule {
    pub abi_version: u32,
    pub module: String,
    pub layouts: Vec<BoundaryLayout>,
    pub symbols: Vec<BoundarySymbol>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub allocators: Vec<BoundaryAllocator>,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub layout_hash: String,
}

impl Default for BoundaryModule {
    fn default() -> Self {
        Self {
            abi_version: IN_ABI_VERSION,
            module: String::new(),
            layouts: Vec::new(),
            symbols: Vec::new(),
            allocators: Vec::new(),
            layout_hash: String::new(),
        }
    }
}

impl BoundaryModule {
    pub fn compute_layout_hash(&self) -> String {
        let mut payload = serde_json::Map::new();
        let layouts = serde_json::to_value(&self.layouts).unwrap_or(serde_json::Value::Null);
        let symbols = serde_json::to_value(&self.symbols).unwrap_or(serde_json::Value::Null);
        payload.insert("layouts".to_string(), layouts);
        payload.insert("symbols".to_string(), symbols);
        let canonical = serde_json::to_string(&payload).unwrap_or_default();
        let hash = blake3::hash(canonical.as_bytes());
        format!("blake3-{}", hash.to_hex())
    }

    pub fn with_layout_hash(mut self) -> Self {
        self.layout_hash = self.compute_layout_hash();
        self
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompileArtifact {
    pub semantic: UnifiedModule,
    pub boundary: Option<BoundaryModule>,
}

impl CompileArtifact {
    #[must_use]
    pub fn from_semantic(semantic: UnifiedModule) -> Self {
        Self {
            semantic,
            boundary: None,
        }
    }

    #[must_use]
    pub fn with_boundary(semantic: UnifiedModule, boundary: BoundaryModule) -> Self {
        Self {
            semantic,
            boundary: Some(boundary),
        }
    }
}

// ── Component Metadata (generic SCI-like sidecar) ──────────────

/// Code section descriptor for a compiled component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeSection {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub flags: String,
}

/// Data section descriptor for a compiled component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DataSection {
    pub name: String,
    pub offset: u64,
    pub size: u64,
    pub flags: String,
}

/// Capability declaration in component metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CapabilityDecl {
    pub name: String,
    pub capability_type: String,
    pub args: Vec<String>,
}

/// Object schema (struct layout) referenced by a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectSchema {
    pub name: String,
    pub fields: Vec<ObjectField>,
    pub size: u64,
    pub align: u64,
}

/// Field within an object schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ObjectField {
    pub name: String,
    #[serde(rename = "type")]
    pub typ: String,
    pub offset: u64,
    pub size: u64,
}

/// Memory requirements for a component.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MemoryRequirements {
    pub stack: u64,
    pub heap: u64,
    pub static_data: u64,
}

/// Build provenance metadata.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    pub compiler: String,
    pub compiler_version: String,
    pub source_hash: String,
}

/// Service import declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceImport {
    pub name: String,
    pub interface: String,
}

/// Service export declaration.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ServiceExport {
    pub name: String,
    pub interface: String,
}

/// Generic component metadata sidecar.
///
/// Emitted alongside compiled artifacts when the source contains
/// component declarations. This is the generic metadata shape
/// that SCI (Space Component Image) and other component-loading
/// contracts can consume.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ComponentMetadata {
    pub component: String,
    pub target: String,
    pub entry: Option<String>,
    pub code_sections: Vec<CodeSection>,
    pub data_sections: Vec<DataSection>,
    pub imports: Vec<ServiceImport>,
    pub exports: Vec<ServiceExport>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities_required: Vec<CapabilityDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub capabilities_exported: Vec<CapabilityDecl>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub object_schemas: Vec<ObjectSchema>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub memory: Option<MemoryRequirements>,
    pub checkpoint: String,
    pub deterministic: bool,
    pub provenance: Provenance,
}
