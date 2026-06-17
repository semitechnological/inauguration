use crate::core_ir::Span;
use std::fmt;

/// Compiler error with category and optional source position.
#[derive(Debug, Clone)]
pub struct CompileError {
    pub category: ErrorCategory,
    pub message: String,
    pub span: Option<Span>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErrorCategory {
    Parse,
    TypeError,
    Verifier,
    Lower,
    Io,
    Internal,
}

impl CompileError {
    pub fn parse(msg: impl Into<String>) -> Self {
        Self {
            category: ErrorCategory::Parse,
            message: msg.into(),
            span: None,
        }
    }
    pub fn parse_at(line: u32, col: u32, file: &str, msg: impl Into<String>) -> Self {
        Self {
            category: ErrorCategory::Parse,
            message: msg.into(),
            span: Some(Span::new(line, col, file)),
        }
    }
    pub fn type_error(msg: impl Into<String>) -> Self {
        Self {
            category: ErrorCategory::TypeError,
            message: msg.into(),
            span: None,
        }
    }
    pub fn verifier(msg: impl Into<String>) -> Self {
        Self {
            category: ErrorCategory::Verifier,
            message: msg.into(),
            span: None,
        }
    }
    pub fn lower(msg: impl Into<String>) -> Self {
        Self {
            category: ErrorCategory::Lower,
            message: msg.into(),
            span: None,
        }
    }
    pub fn io(msg: impl Into<String>) -> Self {
        Self {
            category: ErrorCategory::Io,
            message: msg.into(),
            span: None,
        }
    }
    pub fn internal(msg: impl Into<String>) -> Self {
        Self {
            category: ErrorCategory::Internal,
            message: msg.into(),
            span: None,
        }
    }
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let cat = match self.category {
            ErrorCategory::Parse => "parse error",
            ErrorCategory::TypeError => "type error",
            ErrorCategory::Verifier => "verifier error",
            ErrorCategory::Lower => "lowering error",
            ErrorCategory::Io => "I/O error",
            ErrorCategory::Internal => "internal error",
        };
        if let Some(span) = &self.span {
            write!(
                f,
                "{cat} at {}:{}:{}: {}",
                span.file, span.line, span.col, self.message
            )
        } else {
            write!(f, "{cat}: {}", self.message)
        }
    }
}

impl std::error::Error for CompileError {}

impl From<String> for CompileError {
    fn from(s: String) -> Self {
        CompileError::internal(s)
    }
}

impl From<&str> for CompileError {
    fn from(s: &str) -> Self {
        CompileError::internal(s.to_string())
    }
}

impl From<std::io::Error> for CompileError {
    fn from(e: std::io::Error) -> Self {
        CompileError::io(e.to_string())
    }
}

/// Convenience alias.
pub type CompileResult<T> = Result<T, CompileError>;
