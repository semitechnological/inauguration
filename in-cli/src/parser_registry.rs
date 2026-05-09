//! Parser selection for `in build`: CLI and env override extension-based resolution.
//!
//! ## Precedence (narrower wins first)
//!
//! 1. **`--parser in`**: always selects the `.in` v0 front ([`ParserCli::In`]).
//! 2. **`--parser auto`** and the path is an **existing regular file**: read the first line.
//!    If it is exactly `#!in parser=in` or `#!in parser=auto` (after trimming trailing
//!    newline / `\r`), then:
//!    - `parser=in` → `.in` front (overrides file extension and `IN_PARSER`).
//!    - `parser=auto` → continue with normal `auto` rules below (extension and `IN_PARSER`
//!      apply).
//! 3. **`IN_PARSER=in`** (case-insensitive): `.in` front.
//! 4. **`.in` extension**: `.in` front.
//! 5. Otherwise: Swift SIL emit path.

use crate::core_ir::UnifiedModule;
use crate::in_lang_parse;
use clap::ValueEnum;
use std::io::{BufRead, BufReader};
use std::path::Path;
use thiserror::Error;

/// Identifies an in-tree source front (extend for Python/Ruby, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParserId {
    /// `.in` v0 line-oriented grammar.
    In,
    /// Reserved for a future Python front (not wired into resolution yet).
    Python,
    /// Reserved for a future Ruby front (not wired into resolution yet).
    Ruby,
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

const MAGIC_SHEBANG_PREFIX: &str = "#!in ";
const MAGIC_PARSER_KEY: &str = "parser=";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MagicParserDirective {
    ForceIn,
    /// Hand control back to normal `auto` resolution (extension + `IN_PARSER`).
    DeferAuto,
}

/// Reads the first line of `path` when it is a regular file; returns `None` for
/// directories, symlinks to dirs, missing paths, or I/O errors (fail-open to legacy behavior).
fn read_magic_parser_directive(path: &Path) -> Option<MagicParserDirective> {
    if !path.is_file() {
        return None;
    }
    let file = std::fs::File::open(path).ok()?;
    let mut reader = BufReader::new(file);
    let mut line = String::new();
    reader.read_line(&mut line).ok()?;
    parse_magic_parser_first_line(&line)
}

fn parse_magic_parser_first_line(line: &str) -> Option<MagicParserDirective> {
    let s = line.trim_end_matches(['\r', '\n']);
    let rest = s.strip_prefix(MAGIC_SHEBANG_PREFIX)?;
    let value = rest.strip_prefix(MAGIC_PARSER_KEY)?;
    if value == "in" {
        Some(MagicParserDirective::ForceIn)
    } else if value == "auto" {
        Some(MagicParserDirective::DeferAuto)
    } else {
        None
    }
}

pub fn resolve_parser_id(path: &Path, cli: ParserCli) -> ResolvedBuildParser {
    match cli {
        ParserCli::In => ResolvedBuildParser::InLang(ParserId::In),
        ParserCli::Auto => {
            if let Some(MagicParserDirective::ForceIn) = read_magic_parser_directive(path) {
                return ResolvedBuildParser::InLang(ParserId::In);
            }
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
        ResolvedBuildParser::InLang(ParserId::Python) => Err(ParserRegistryError::Msg(
            "parser front `python` is not implemented".to_string(),
        )),
        ResolvedBuildParser::InLang(ParserId::Ruby) => Err(ParserRegistryError::Msg(
            "parser front `ruby` is not implemented".to_string(),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::Path;
    use std::sync::Mutex;

    static ENV_LOCK: Mutex<()> = Mutex::new(());

    fn temp_file_path(suffix: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "inauguration-parser-registry-{}-{}-{suffix}",
            std::process::id(),
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn parse_magic_first_line_recognizes_in_and_auto() {
        assert_eq!(
            parse_magic_parser_first_line("#!in parser=in\n"),
            Some(MagicParserDirective::ForceIn)
        );
        assert_eq!(
            parse_magic_parser_first_line("#!in parser=auto\r\n"),
            Some(MagicParserDirective::DeferAuto)
        );
        assert_eq!(parse_magic_parser_first_line("#!in parser=swift\n"), None);
        assert_eq!(parse_magic_parser_first_line("#!/usr/bin/env in\n"), None);
    }

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
    fn magic_parser_in_overrides_non_in_extension() {
        let path = temp_file_path("magic.swift");
        std::fs::write(
            &path,
            "#!in parser=in\nfn main() -> void\n",
        )
        .expect("write temp");
        assert!(matches!(
            resolve_parser_id(&path, ParserCli::Auto),
            ResolvedBuildParser::InLang(ParserId::In)
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn magic_parser_auto_defers_to_in_parser_env() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        let path = temp_file_path("defer.swift");
        std::fs::write(
            &path,
            "#!in parser=auto\nfn main() -> void\n",
        )
        .expect("write temp");
        // Rust 2024: mutating process environment is `unsafe` (see `set_var` docs).
        unsafe {
            std::env::set_var("IN_PARSER", "in");
        }
        let resolved = resolve_parser_id(&path, ParserCli::Auto);
        unsafe {
            std::env::remove_var("IN_PARSER");
        }
        let _ = std::fs::remove_file(&path);
        assert!(matches!(
            resolved,
            ResolvedBuildParser::InLang(ParserId::In)
        ));
    }

    #[test]
    fn magic_parser_auto_defers_to_dot_in_extension() {
        let path = temp_file_path("defer.in");
        std::fs::write(
            &path,
            "#!in parser=auto\nfn main() -> void\n",
        )
        .expect("write temp");
        assert!(matches!(
            resolve_parser_id(&path, ParserCli::Auto),
            ResolvedBuildParser::InLang(ParserId::In)
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unknown_magic_parser_value_falls_through_to_extension() {
        let path = temp_file_path("unknown.swift");
        std::fs::write(&path, "#!in parser=nope\n").expect("write temp");
        assert!(matches!(
            resolve_parser_id(&path, ParserCli::Auto),
            ResolvedBuildParser::SwiftSilEmit
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stub_python_front_errors_with_stable_message() {
        let path = temp_file_path("stub.py");
        std::fs::write(&path, "").ok();
        let err = parse_with_resolved(ResolvedBuildParser::InLang(ParserId::Python), &path)
            .expect_err("stub python");
        assert_eq!(
            err.to_string(),
            "parser front `python` is not implemented"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stub_ruby_front_errors_with_stable_message() {
        let path = temp_file_path("stub.rb");
        std::fs::write(&path, "").ok();
        let err = parse_with_resolved(ResolvedBuildParser::InLang(ParserId::Ruby), &path)
            .expect_err("stub ruby");
        assert_eq!(
            err.to_string(),
            "parser front `ruby` is not implemented"
        );
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parse_with_resolved_reads_minimal_in_file() {
        let path = temp_file_path("hello.in");
        std::fs::write(&path, "fn main() -> void\n").expect("write temp .in");
        let m = parse_with_resolved(ResolvedBuildParser::InLang(ParserId::In), &path)
            .expect("parse")
            .expect("module");
        let _ = std::fs::remove_file(&path);
        assert!(!m.decls.is_empty());
    }
}
