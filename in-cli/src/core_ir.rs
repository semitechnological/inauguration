//! Cross-frontend core AST (v0). Bodies may be empty until a frontend fills statements.

pub use crate::swift_subset::{Stmt, Typ};

/// Single-module view produced by language fronts before lowering to textual SIL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedModule {
    pub decls: Vec<Decl>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decl {
    Struct {
        name: String,
        fields: Vec<(String, Typ)>,
    },
    Function {
        name: String,
        params: Vec<(String, Typ)>,
        ret: Typ,
        body: Vec<Stmt>,
    },
}
