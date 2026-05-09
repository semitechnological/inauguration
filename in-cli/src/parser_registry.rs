//! Parser selection for `in build`: CLI and env override extension-based resolution.

use crate::core_ir::UnifiedModule;
use crate::in_lang_parse;
use clap::ValueEnum;
use std::path::Path;
use thiserror::Error;

/// Identifies an in-tree source front (extend for Python/Ruby, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParserId {
    /// `.in` v0 line-oriented grammar.
    In,
}

/// CLI `--parser`: `auto` (default) or `in` (see `IN_PARSER=in` env override).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, ValueEnum)]
pub enum ParserCli {
    #[default]
    Auto,
    In,
}

/// Resolved frontend for `in build`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResolvedBuildParser {
    InLang(ParserId),
    /// Swift gather + `sil_emit::emit_textual_sil` (`swiftc` or subset env).
    SwiftSilEmit,
}

fn env_forces_in_parser() -> bool {
    matches!(
        std::env::var("IN_PARSER").ok().as_deref(),
        Some(s) if s.eq_ignore_ascii_case("in")
    )
}

/// Resolution order: `--parser in` / `IN_PARSER=in` → In; `auto` + `.in` extension → In; else Swift SIL emit.
/// Future: magic-line stub in source before extension fallback.
pub fn resolve_parser_id(path: &Path, cli: ParserCli) -> ResolvedBuildParser {
    match cli {
        ParserCli::In => ResolvedBuildParser::InLang(ParserId::In),
        ParserCli::Auto => {
            if env_forces_in_parser() {
                return ResolvedBuildParser::InLang(ParserId::In);
            }
            if path.extension().and_then(|s| s.to_str()) == Some("in") {
                ResolvedBuildParser::InLang(ParserId::In)
            } else {
                ResolvedBuildParser::SwiftSilEmit
            }
        }
    }
}

#[derive(Debug, Error)]
pub enum ParserRegistryError {
    #[error("{0}")]
    Msg(String),
}

/// Maps a path to core IR via the selected front.
pub trait SourceParser {
    fn parse_to_core(&self, path: &Path) -> Result<UnifiedModule, ParserRegistryError>;
}

/// `.in` v0 parser adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct InLangParser;

impl SourceParser for InLangParser {
    fn parse_to_core(&self, path: &Path) -> Result<UnifiedModule, ParserRegistryError> {
        in_lang_parse::parse_in_file(path).map_err(ParserRegistryError::Msg)
    }
}

/// Dispatch by [`ResolvedBuildParser`].
pub fn parse_with_resolved(
    resolved: ResolvedBuildParser,
    path: &Path,
) -> Result<Option<UnifiedModule>, ParserRegistryError> {
    match resolved {
        ResolvedBuildParser::SwiftSilEmit => Ok(None),
        ResolvedBuildParser::InLang(ParserId::In) => InLangParser.parse_to_core(path).map(Some),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;

    #[test]
    fn auto_resolves_in_extension() {
        assert!(matches!(
            resolve_parser_id(Path::new("hello.in"), ParserCli::Auto),
            ResolvedBuildParser::InLang(ParserId::In)
        ));
    }

    #[test]
    fn auto_swift_for_swift() {
        assert!(matches!(
            resolve_parser_id(Path::new("App.swift"), ParserCli::Auto),
            ResolvedBuildParser::SwiftSilEmit
        ));
    }

    #[test]
    fn parse_with_resolved_reads_minimal_in_file() {
        let path = std::env::temp_dir().join(format!(
            "inauguration-parse-test-{}-{}.in",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        std::fs::write(&path, "fn main() -> void\n").expect("write temp .in");
        let m = parse_with_resolved(ResolvedBuildParser::InLang(ParserId::In), &path)
            .expect("parse")
            .expect("module");
        let _ = std::fs::remove_file(&path);
        assert!(!m.decls.is_empty());
    }
}
