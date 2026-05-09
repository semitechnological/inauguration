//! Parser selection for `in build`: CLI, env, magic first line, and file extension.
//!
//! ## Precedence (narrower wins first)
//!
//! 1. **`--parser in`**: always selects the `.in` v0 front ([`ParserId::In`]).
//! 2. **`--parser auto`** and the path is an **existing regular file**: read the first line.
//!    If it starts with `#!in parser=` (after trimming):
//!    - `parser=in` → `.in` front (overrides extension and `IN_PARSER`).
//!    - `parser=auto` → continue with normal `auto` rules below.
//!    - `parser=<slug>` for any other known slug → that front (often a **stub** until lowered to
//!      Core IR); see [`ParserId::as_str`] and [parser-surface.md](../../docs/architecture/parser-surface.md).
//! 3. **`IN_PARSER=in`** (case-insensitive): `.in` front.
//! 4. **Known extension** (`.in`, `.java`, `.cpp`, `.py`, …): [`ResolvedBuildParser::CoreIr`]
//!    with the matching [`ParserId`] (only [`ParserId::In`] is implemented).
//! 5. Otherwise: Swift SIL emit path (`swiftc` or subset env).

use crate::core_ir::UnifiedModule;
use crate::in_lang_parse;
use clap::ValueEnum;
use std::io::{BufRead, BufReader};
use std::fmt;
use std::path::Path;

/// In-tree source front identifier. Most variants are **stubs** today: only [`ParserId::In`]
/// parses to [`UnifiedModule`]; everything else returns [`ParserRegistryError::NotImplemented`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ParserId {
    /// `.in` v0 — implemented.
    In,
    // C family
    C,
    Cpp,
    ObjC,
    ObjCpp,
    // JVM / CLR
    Java,
    Kotlin,
    Scala,
    CSharp,
    FSharp,
    VbNet,
    // Dynamic OO / scripting
    Python,
    Ruby,
    Php,
    Perl,
    // Web / JS-shaped
    JavaScript,
    TypeScript,
    // Systems / other curly-brace
    Go,
    Rust,
    Zig,
    Dart,
    Lua,
    // More OO / FP fronts (stubs)
    Clojure,
    Groovy,
    Elixir,
    Erlang,
    Haskell,
    Julia,
    R,
    Nim,
    D,
    Crystal,
}

impl ParserId {
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            ParserId::In => "in",
            ParserId::C => "c",
            ParserId::Cpp => "cpp",
            ParserId::ObjC => "objc",
            ParserId::ObjCpp => "objc++",
            ParserId::Java => "java",
            ParserId::Kotlin => "kotlin",
            ParserId::Scala => "scala",
            ParserId::CSharp => "csharp",
            ParserId::FSharp => "fsharp",
            ParserId::VbNet => "vb",
            ParserId::Python => "python",
            ParserId::Ruby => "ruby",
            ParserId::Php => "php",
            ParserId::Perl => "perl",
            ParserId::JavaScript => "javascript",
            ParserId::TypeScript => "typescript",
            ParserId::Go => "go",
            ParserId::Rust => "rust",
            ParserId::Zig => "zig",
            ParserId::Dart => "dart",
            ParserId::Lua => "lua",
            ParserId::Clojure => "clojure",
            ParserId::Groovy => "groovy",
            ParserId::Elixir => "elixir",
            ParserId::Erlang => "erlang",
            ParserId::Haskell => "haskell",
            ParserId::Julia => "julia",
            ParserId::R => "r",
            ParserId::Nim => "nim",
            ParserId::D => "d",
            ParserId::Crystal => "crystal",
        }
    }

    #[must_use]
    pub const fn family_label(self) -> &'static str {
        match self {
            ParserId::In => "inauguration .in",
            ParserId::C | ParserId::Cpp | ParserId::ObjC | ParserId::ObjCpp => "C-like",
            ParserId::Java | ParserId::Kotlin | ParserId::Scala => "JVM / class-based",
            ParserId::CSharp | ParserId::FSharp | ParserId::VbNet => ".NET",
            ParserId::Python | ParserId::Ruby | ParserId::Php | ParserId::Perl => "dynamic OO / scripting",
            ParserId::JavaScript | ParserId::TypeScript => "ECMAScript-shaped",
            ParserId::Go | ParserId::Rust | ParserId::Zig => "systems / curly-brace",
            ParserId::Dart | ParserId::Lua => "OO / embeddable",
            ParserId::Clojure | ParserId::Elixir | ParserId::Erlang | ParserId::Haskell => "functional",
            ParserId::Groovy => "JVM scripting",
            ParserId::Julia | ParserId::R => "numeric / scientific",
            ParserId::Nim | ParserId::D | ParserId::Crystal => "ALGOL-descended",
        }
    }
}

/// Map a **lowercase** extension (no leading dot) to a tracked front. `swift` is intentionally absent
/// so the Swift toolchain path handles `.swift`.
#[must_use]
pub fn parser_id_from_extension(ext: &str) -> Option<ParserId> {
    match ext {
        "in" => Some(ParserId::In),
        "c" | "h" => Some(ParserId::C),
        "cc" | "cpp" | "cxx" | "hpp" | "hxx" | "hh" | "h++" | "ipp" => Some(ParserId::Cpp),
        "m" => Some(ParserId::ObjC),
        "mm" => Some(ParserId::ObjCpp),
        "java" => Some(ParserId::Java),
        "kt" | "kts" => Some(ParserId::Kotlin),
        "scala" | "sc" => Some(ParserId::Scala),
        "cs" => Some(ParserId::CSharp),
        "fs" | "fsx" | "fsi" => Some(ParserId::FSharp),
        "vb" => Some(ParserId::VbNet),
        "py" | "pyi" | "pyw" => Some(ParserId::Python),
        "rb" | "rake" | "gemspec" => Some(ParserId::Ruby),
        "php" | "phtml" => Some(ParserId::Php),
        "pl" | "pm" => Some(ParserId::Perl),
        "js" | "mjs" | "cjs" | "jsx" => Some(ParserId::JavaScript),
        "ts" | "tsx" | "mts" | "cts" => Some(ParserId::TypeScript),
        "go" => Some(ParserId::Go),
        "rs" => Some(ParserId::Rust),
        "zig" => Some(ParserId::Zig),
        "dart" => Some(ParserId::Dart),
        "lua" => Some(ParserId::Lua),
        "clj" | "cljs" | "cljc" => Some(ParserId::Clojure),
        "groovy" => Some(ParserId::Groovy),
        "ex" | "exs" => Some(ParserId::Elixir),
        "erl" | "hrl" => Some(ParserId::Erlang),
        "hs" | "lhs" => Some(ParserId::Haskell),
        "jl" => Some(ParserId::Julia),
        "r" => Some(ParserId::R),
        "nim" => Some(ParserId::Nim),
        "d" => Some(ParserId::D),
        "cr" => Some(ParserId::Crystal),
        _ => None,
    }
}

/// Map a magic-line token (already trimmed) after `parser=`. Does not handle `in` / `auto`
/// (those are handled before this is called).
#[must_use]
pub fn parser_id_from_magic_token(token: &str) -> Option<ParserId> {
    let t = token.trim();
    parser_id_from_extension(&t.to_ascii_lowercase()).or_else(|| {
        match t.to_ascii_lowercase().as_str() {
            "objc" | "objective-c" => Some(ParserId::ObjC),
            "objc++" | "objcpp" => Some(ParserId::ObjCpp),
            "csharp" => Some(ParserId::CSharp),
            "fsharp" => Some(ParserId::FSharp),
            "kotlin" => Some(ParserId::Kotlin),
            "typescript" => Some(ParserId::TypeScript),
            "javascript" => Some(ParserId::JavaScript),
            "cplusplus" | "c++" => Some(ParserId::Cpp),
            _ => None,
        }
    })
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
    /// Core IR path (implemented: [`ParserId::In`]; stubs: all other [`ParserId`]).
    CoreIr(ParserId),
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
    UseParser(ParserId),
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
        return Some(MagicParserDirective::ForceIn);
    }
    if value == "auto" {
        return Some(MagicParserDirective::DeferAuto);
    }
    let id = parser_id_from_magic_token(value)?;
    Some(MagicParserDirective::UseParser(id))
}

pub fn resolve_parser_id(path: &Path, cli: ParserCli) -> ResolvedBuildParser {
    match cli {
        ParserCli::In => ResolvedBuildParser::CoreIr(ParserId::In),
        ParserCli::Auto => {
            if let Some(m) = read_magic_parser_directive(path) {
                match m {
                    MagicParserDirective::ForceIn => {
                        return ResolvedBuildParser::CoreIr(ParserId::In);
                    }
                    MagicParserDirective::UseParser(ParserId::In) => {
                        return ResolvedBuildParser::CoreIr(ParserId::In);
                    }
                    MagicParserDirective::UseParser(id) => {
                        return ResolvedBuildParser::CoreIr(id);
                    }
                    MagicParserDirective::DeferAuto => {}
                }
            }
            if env_forces_in_parser() {
                return ResolvedBuildParser::CoreIr(ParserId::In);
            }
            if let Some(ext) = path.extension().and_then(|s| s.to_str()) {
                let el = ext.to_ascii_lowercase();
                if let Some(id) = parser_id_from_extension(&el) {
                    return ResolvedBuildParser::CoreIr(id);
                }
            }
            ResolvedBuildParser::SwiftSilEmit
        }
    }
}

#[derive(Debug)]
pub enum ParserRegistryError {
    Msg(String),
    NotImplemented(ParserId),
}

impl fmt::Display for ParserRegistryError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ParserRegistryError::Msg(s) => write!(f, "{s}"),
            ParserRegistryError::NotImplemented(id) => write!(
                f,
                "parser front `{}` ({}) is not implemented — only `.in` lowers to Core IR today; see docs/architecture/parser-surface.md",
                id.as_str(),
                id.family_label()
            ),
        }
    }
}

impl std::error::Error for ParserRegistryError {}

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
        ResolvedBuildParser::CoreIr(ParserId::In) => {
            InLangParser.parse_to_core(path).map(Some)
        }
        ResolvedBuildParser::CoreIr(id) => Err(ParserRegistryError::NotImplemented(id)),
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
        assert_eq!(
            parse_magic_parser_first_line("#!in parser=java\n"),
            Some(MagicParserDirective::UseParser(ParserId::Java))
        );
        assert_eq!(parse_magic_parser_first_line("#!in parser=nope\n"), None);
        assert_eq!(parse_magic_parser_first_line("#!/usr/bin/env in\n"), None);
    }

    #[test]
    fn auto_resolves_in_extension() {
        assert!(matches!(
            resolve_parser_id(Path::new("hello.in"), ParserCli::Auto),
            ResolvedBuildParser::CoreIr(ParserId::In)
        ));
    }

    #[test]
    fn auto_resolves_java_extension_to_stub() {
        assert!(matches!(
            resolve_parser_id(Path::new("Foo.java"), ParserCli::Auto),
            ResolvedBuildParser::CoreIr(ParserId::Java)
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
    fn auto_cpp_for_cc() {
        assert!(matches!(
            resolve_parser_id(Path::new("lib.cc"), ParserCli::Auto),
            ResolvedBuildParser::CoreIr(ParserId::Cpp)
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
            ResolvedBuildParser::CoreIr(ParserId::In)
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
            ResolvedBuildParser::CoreIr(ParserId::In)
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
            ResolvedBuildParser::CoreIr(ParserId::In)
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn unknown_magic_parser_value_falls_through_to_extension() {
        let _lock = ENV_LOCK.lock().expect("env lock");
        unsafe {
            std::env::remove_var("IN_PARSER");
        }
        let path = temp_file_path("unknown.swift");
        std::fs::write(&path, "#!in parser=nope\n").expect("write temp");
        assert!(matches!(
            resolve_parser_id(&path, ParserCli::Auto),
            ResolvedBuildParser::SwiftSilEmit
        ));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn stub_java_front_errors_with_stable_variant() {
        let path = temp_file_path("stub.java");
        std::fs::write(&path, "").ok();
        let err = parse_with_resolved(ResolvedBuildParser::CoreIr(ParserId::Java), &path)
            .expect_err("stub java");
        assert!(matches!(err, ParserRegistryError::NotImplemented(ParserId::Java)));
        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn parse_with_resolved_reads_minimal_in_file() {
        let path = temp_file_path("hello.in");
        std::fs::write(&path, "fn main() -> void\n").expect("write temp .in");
        let m = parse_with_resolved(ResolvedBuildParser::CoreIr(ParserId::In), &path)
            .expect("parse")
            .expect("module");
        let _ = std::fs::remove_file(&path);
        assert!(!m.decls.is_empty());
    }
}
