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