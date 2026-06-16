//! Cross-frontend core AST (v0). Bodies may be empty until a frontend fills statements.

use serde::{Deserialize, Serialize};

/// Source position span.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Span {
    pub line: u32,
    pub col: u32,
    pub file: String,
}

impl Span {
    #[must_use]
    pub fn new(line: u32, col: u32, file: &str) -> Self {
        Self { line, col, file: file.to_string() }
    }
    #[must_use]
    pub fn unknown() -> Self {
        Self { line: 0, col: 0, file: String::new() }
    }
}

impl std::fmt::Display for Span {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        if self.file.is_empty() {
            write!(f, "{}:{}", self.line, self.col)
        } else {
            write!(f, "{}:{}:{}", self.file, self.line, self.col)
        }
    }
}

/// Source position attached to a Core IR node.
pub type NodeSpan = Option<Span>;


#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct FloatVal(pub f64);

impl PartialEq for FloatVal {
    fn eq(&self, other: &Self) -> bool {
        self.0.to_bits() == other.0.to_bits()
    }
}
impl Eq for FloatVal {}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Typ {
    Int,
    String,
    Bool,
    Float,
    Void,
    Array(Box<Typ>),
    Named(String),
    Generic(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Expr {
    IntLit(i64),
    FloatLit(FloatVal),
    StringLit(String),
    BoolLit(bool),
    Ident(String),
    Unary {
        op: String,
        expr: Box<Expr>,
    },
    Binary {
        op: String,
        lhs: Box<Expr>,
        rhs: Box<Expr>,
    },
    StructInit {
        name: String,
        fields: Vec<(String, Expr)>,
    },
    Field {
        base: Box<Expr>,
        name: String,
    },
    ArrayLit(Vec<Expr>),
    Index {
        base: Box<Expr>,
        index: Box<Expr>,
    },
    Call {
        callee: Box<Expr>,
        args: Vec<Expr>,
    },
    Closure {
        params: Vec<(String, Typ)>,
        ret: Typ,
        body: Vec<Stmt>,
        captures: Vec<String>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Stmt {
    Let(String, Option<Typ>, Expr),
    Assign(String, Expr),
    IndexAssign {
        base: Expr,
        index: Expr,
        value: Expr,
    },
    Return(Option<Expr>),
    If {
        cond: Expr,
        then_body: Vec<Stmt>,
        else_body: Vec<Stmt>,
    },
    Loop {
        kind: LoopKind,
        cond: Option<Expr>,
        body: Vec<Stmt>,
    },
    Match {
        scrutinee: Expr,
        arms: Vec<MatchArm>,
    },
    Throw(Expr),
    Try {
        body: Vec<Stmt>,
        catches: Vec<CatchArm>,
    },
    /// Evaluated for side effects (e.g. `.in` expression statements).
    Expr(Expr),
    /// Break out of the current loop.
    Break,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LoopKind {
    For,
    While,
    Infinite,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum MatchPattern {
    IntPat(i64),
    StringPat(String),
    BoolPat(bool),
    WildPat,
    IdentPat(String),
    RestPat,
    TuplePat(Vec<MatchPattern>),
    StructPat {
        name: String,
        fields: Vec<(String, MatchPattern)>,
    },
    ArrayPat(Vec<MatchPattern>),
}

fn trim_match_pat(s: &str) -> &str {
    s.trim()
}

fn split_match_pat_args(inner: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut start = 0usize;
    let mut depth = 0i32;
    for (i, c) in inner.char_indices() {
        match c {
            '(' | '{' | '[' => depth += 1,
            ')' | '}' | ']' => depth -= 1,
            ',' if depth == 0 => {
                let arg = trim_match_pat(&inner[start..i]);
                if !arg.is_empty() {
                    out.push(arg.to_string());
                }
                start = i + 1;
            }
            _ => {}
        }
    }
    let tail = trim_match_pat(&inner[start..]);
    if !tail.is_empty() {
        out.push(tail.to_string());
    }
    out
}

impl MatchPattern {
    pub fn parse(s: &str) -> Result<Self, String> {
        let s = trim_match_pat(s).trim_end_matches(':').trim();
        let s = s.strip_prefix("case ").unwrap_or(s).trim();
        if s.is_empty() {
            return Err(".in: empty pattern".into());
        }
        if s == "_" || s == "else" || s == "default" {
            return Ok(MatchPattern::WildPat);
        }
        if s == ".." {
            return Ok(MatchPattern::RestPat);
        }
        if s == "true" {
            return Ok(MatchPattern::BoolPat(true));
        }
        if s == "false" {
            return Ok(MatchPattern::BoolPat(false));
        }
        if let Ok(n) = s.parse::<i64>() {
            return Ok(MatchPattern::IntPat(n));
        }
        if s.len() >= 2 && s.starts_with('"') && s.ends_with('"') {
            return Ok(MatchPattern::StringPat(s[1..s.len() - 1].to_string()));
        }
        if s.starts_with('(') && s.ends_with(')') {
            let inner = &s[1..s.len() - 1];
            let parts = split_match_pat_args(inner);
            let pats: Result<Vec<_>, _> = parts.iter().map(|p| MatchPattern::parse(p)).collect();
            return Ok(MatchPattern::TuplePat(pats?));
        }
        if s.starts_with('[') && s.ends_with(']') {
            let inner = &s[1..s.len() - 1];
            let parts = split_match_pat_args(inner);
            let pats: Result<Vec<_>, _> = parts.iter().map(|p| MatchPattern::parse(p)).collect();
            return Ok(MatchPattern::ArrayPat(pats?));
        }
        if let Some(open) = s.find('{')
            && s.ends_with('}')
        {
            let name = trim_match_pat(&s[..open]);
            if !name.is_empty() && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
                let inner = &s[open + 1..s.len() - 1];
                let field_strs = split_match_pat_args(inner);
                let mut fields = Vec::new();
                for f in field_strs {
                    if let Some((field_name, field_pat)) = f.split_once(':') {
                        let fn_trim = trim_match_pat(field_name);
                        let fp_trim = trim_match_pat(field_pat);
                        if fn_trim.is_empty() {
                            return Err(format!(".in: empty field name in struct pattern `{s}`"));
                        }
                        fields.push((fn_trim.to_string(), MatchPattern::parse(fp_trim)?));
                    } else {
                        let fn_trim = trim_match_pat(&f);
                        if fn_trim.is_empty() {
                            return Err(format!(".in: empty field name in struct pattern `{s}`"));
                        }
                        fields.push((
                            fn_trim.to_string(),
                            MatchPattern::IdentPat(fn_trim.to_string()),
                        ));
                    }
                }
                return Ok(MatchPattern::StructPat {
                    name: name.to_string(),
                    fields,
                });
            }
        }
        if s.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
            return Ok(MatchPattern::IdentPat(s.to_string()));
        }
        Err(format!(".in: unknown pattern `{s}`"))
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MatchArm {
    pub pattern: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CatchArm {
    pub pattern: String,
    pub body: Vec<Stmt>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Visibility {
    Pub,
    Private,
    Internal,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Import {
    pub path: String,
    pub alias: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MethodSig {
    pub name: String,
    pub params: Vec<(String, Typ)>,
    pub ret: Typ,
}

/// Single-module view produced by language fronts before lowering to textual SIL.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UnifiedModule {
    pub identity: CoreModuleIdentity,
    pub decls: Vec<Decl>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct CoreModuleIdentity {
    pub package: Option<String>,
    pub module: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModuleIdentityReport {
    pub package: Option<String>,
    pub module: Option<String>,
    pub requested_module_id: String,
    pub effective_module_id: String,
}

impl UnifiedModule {
    #[must_use]
    pub fn new(decls: Vec<Decl>) -> Self {
        Self {
            identity: CoreModuleIdentity::default(),
            decls,
        }
    }

    #[must_use]
    pub fn with_identity(decls: Vec<Decl>, identity: CoreModuleIdentity) -> Self {
        Self { identity, decls }
    }

    #[must_use]
    pub fn effective_module_id<'a>(&'a self, requested: &'a str) -> &'a str {
        if requested != "App" {
            return requested;
        }
        self.identity
            .module
            .as_deref()
            .or(self.identity.package.as_deref())
            .unwrap_or(requested)
    }

    #[must_use]
    pub fn identity_report(&self, requested: &str) -> ModuleIdentityReport {
        ModuleIdentityReport {
            package: self.identity.package.clone(),
            module: self.identity.module.clone(),
            requested_module_id: requested.to_string(),
            effective_module_id: self.effective_module_id(requested).to_string(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentImport {
    pub name: String,
    pub interface: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentExport {
    pub name: String,
    pub interface: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ComponentCapability {
    pub name: String,
    pub capability_type: String,
    pub args: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decl {
    Struct {
        name: String,
        fields: Vec<(String, Typ)>,
        type_params: Vec<String>,
    },
    Function {
        name: String,
        params: Vec<(String, Typ)>,
        ret: Typ,
        body: Vec<Stmt>,
        type_params: Vec<String>,
    },
    Class {
        name: String,
        fields: Vec<(String, Typ)>,
        methods: Vec<Decl>,
        visibility: Visibility,
        extends: Option<String>,
        implements: Vec<String>,
        type_params: Vec<String>,
    },
    Interface {
        name: String,
        methods: Vec<MethodSig>,
        visibility: Visibility,
        type_params: Vec<String>,
    },
    Component {
        name: String,
        target: String,
        deterministic: bool,
        checkpoint: String,
        imports: Vec<ComponentImport>,
        exports: Vec<ComponentExport>,
        capabilities: Vec<ComponentCapability>,
    },
    /// A global variable or constant declaration.
    /// `mutable` is true for `var`, false for `const`.
    Global {
        name: String,
        typ: Typ,
        init: Option<Box<Expr>>,
        mutable: bool,
    },
}
