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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_error_display() {
        let e = CompileError::parse("unexpected token");
        assert_eq!(format!("{e}"), "parse error: unexpected token");
    }

    #[test]
    fn parse_at_error_display() {
        let e = CompileError::parse_at(10, 5, "test.in", "unexpected token");
        let s = format!("{e}");
        assert!(s.contains("test.in"));
        assert!(s.contains("10"));
        assert!(s.contains("5"));
        assert!(s.contains("unexpected token"));
    }

    #[test]
    fn type_error_display() {
        let e = CompileError::type_error("type mismatch");
        assert_eq!(format!("{e}"), "type error: type mismatch");
    }

    #[test]
    fn verifier_error_display() {
        let e = CompileError::verifier("verification failed");
        assert_eq!(format!("{e}"), "verifier error: verification failed");
    }

    #[test]
    fn lower_error_display() {
        let e = CompileError::lower("lowering failed");
        assert_eq!(format!("{e}"), "lowering error: lowering failed");
    }

    #[test]
    fn io_error_display() {
        let e = CompileError::io("file not found");
        assert_eq!(format!("{e}"), "I/O error: file not found");
    }

    #[test]
    fn internal_error_display() {
        let e = CompileError::internal("bug");
        assert_eq!(format!("{e}"), "internal error: bug");
    }

    #[test]
    fn error_categories() {
        assert_eq!(CompileError::parse("").category, ErrorCategory::Parse);
        assert_eq!(CompileError::type_error("").category, ErrorCategory::TypeError);
        assert_eq!(CompileError::verifier("").category, ErrorCategory::Verifier);
        assert_eq!(CompileError::lower("").category, ErrorCategory::Lower);
        assert_eq!(CompileError::io("").category, ErrorCategory::Io);
        assert_eq!(CompileError::internal("").category, ErrorCategory::Internal);
    }

    #[test]
    fn from_string() {
        let e: CompileError = "some error".to_string().into();
        assert_eq!(e.category, ErrorCategory::Internal);
        assert_eq!(e.message, "some error");
    }

    #[test]
    fn from_str_ref() {
        let e: CompileError = "some error".into();
        assert_eq!(e.category, ErrorCategory::Internal);
    }

    #[test]
    fn from_io_error() {
        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "gone");
        let e: CompileError = io_err.into();
        assert_eq!(e.category, ErrorCategory::Io);
        assert!(e.message.contains("gone"));
    }

    #[test]
    fn compile_error_is_std_error() {
        let e = CompileError::parse("test");
        let _: &dyn std::error::Error = &e;
    }

    #[test]
    fn parse_at_sets_span() {
        let e = CompileError::parse_at(1, 2, "file.in", "err");
        assert!(e.span.is_some());
        let span = e.span.unwrap();
        assert_eq!(span.line, 1);
        assert_eq!(span.col, 2);
        assert_eq!(span.file, "file.in");
    }

    #[test]
    fn constructors_have_no_span() {
        assert!(CompileError::parse("").span.is_none());
        assert!(CompileError::type_error("").span.is_none());
        assert!(CompileError::verifier("").span.is_none());
        assert!(CompileError::lower("").span.is_none());
        assert!(CompileError::io("").span.is_none());
        assert!(CompileError::internal("").span.is_none());
    }
}
