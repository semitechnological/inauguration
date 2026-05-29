use serde::Serialize;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct LanguageSupport {
    pub language: &'static str,
    pub parser_id: Option<&'static str>,
    pub extensions: &'static [&'static str],
    pub level: u8,
    pub level_label: &'static str,
    pub front: &'static str,
    pub runtime_boundary: &'static str,
    pub example: &'static str,
    pub next_step: &'static str,
}

pub const LANGUAGE_SUPPORT: &[LanguageSupport] = &[
    LanguageSupport {
        language: "in",
        parser_id: Some("in"),
        extensions: &["in"],
        level: 3,
        level_label: "bounded Core IR body subset with source diagnostics",
        front: "in_lang_parse",
        runtime_boundary: "self-hosted Core IR to textual SIL and bytecode VM subset",
        example: "apps/polyglot-sample/sample.in",
        next_step: "Bind more package symbols and deepen executable runtime coverage",
    },
    LanguageSupport {
        language: "icore",
        parser_id: Some("icore"),
        extensions: &["icore"],
        level: 2,
        level_label: "versioned Core IR JSON with bounded bodies",
        front: "compiler::icore",
        runtime_boundary: "self-hosted Core IR to textual SIL and bytecode VM subset",
        example: "apps/polyglot-sample/sample.icore",
        next_step: "Keep schema stable and add conformance fixtures from external emitters",
    },
    LanguageSupport {
        language: "Swift",
        parser_id: None,
        extensions: &["swift"],
        level: 2,
        level_label: "Swift subset or swiftc textual SIL path",
        front: "swift_subset, native_swift_sil, sil_emit",
        runtime_boundary: "in-tree subset when IN_NATIVE_SWIFT_SIL=only; optional Swift toolchain fallback",
        example: "apps/polyglot-sample/sample.swift",
        next_step: "Widen the in-tree subset and shrink the swiftc dependency on hot paths",
    },
    LanguageSupport {
        language: "Rust",
        parser_id: Some("rust"),
        extensions: &["rs"],
        level: 2,
        level_label: "dedicated bounded body lowering",
        front: "compiler::rust_front",
        runtime_boundary: "Core IR and textual SIL; rustc is validation only",
        example: "apps/polyglot-sample/sample.rs",
        next_step: "Deepen CFG-aware lowering and remove validation from default success paths",
    },
    LanguageSupport {
        language: "Go",
        parser_id: Some("go"),
        extensions: &["go"],
        level: 2,
        level_label: "dedicated bounded body lowering",
        front: "compiler::go_front",
        runtime_boundary: "Core IR and textual SIL",
        example: "apps/polyglot-sample/sample.go",
        next_step: "Deepen declarations, method sets, packages, and control flow",
    },
    LanguageSupport {
        language: "V",
        parser_id: Some("v"),
        extensions: &["v"],
        level: 2,
        level_label: "dedicated bounded body lowering",
        front: "compiler::v_front",
        runtime_boundary: "Core IR and textual SIL",
        example: "apps/polyglot-sample/sample.v",
        next_step: "Deepen module syntax, structs, and control flow",
    },
    LanguageSupport {
        language: "C",
        parser_id: Some("c"),
        extensions: &["c", "h"],
        level: 2,
        level_label: "Tree-sitter bounded scalar body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; libc/runtime ABI is not bundled",
        example: "apps/polyglot-sample/sample.c",
        next_step: "Add pointer types, declarator metadata, and ABI boundaries",
    },
    LanguageSupport {
        language: "C++",
        parser_id: Some("cpp"),
        extensions: &["cc", "cpp", "cxx", "hpp", "hxx", "hh", "h++", "ipp"],
        level: 2,
        level_label: "Tree-sitter bounded scalar body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; standard library/runtime ABI is not bundled",
        example: "apps/polyglot-sample/sample.cpp",
        next_step: "Add namespaces, methods, templates-as-metadata, and ABI boundaries",
    },
    LanguageSupport {
        language: "Objective-C",
        parser_id: Some("objc"),
        extensions: &["m"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; Objective-C runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add Objective-C method metadata, bounded bodies, and runtime boundary docs",
    },
    LanguageSupport {
        language: "Objective-C++",
        parser_id: Some("objc++"),
        extensions: &["mm"],
        level: 2,
        level_label: "Tree-sitter bounded scalar body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; Objective-C++ runtime/ABI is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add method metadata, C++ interop boundaries, and ABI docs",
    },
    LanguageSupport {
        language: "Java",
        parser_id: Some("java"),
        extensions: &["java"],
        level: 2,
        level_label: "Tree-sitter bounded body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; JVM runtime is not bundled",
        example: "apps/polyglot-sample/Sample.java",
        next_step: "Add class metadata, constructors, fields, and JVM runtime strategy",
    },
    LanguageSupport {
        language: "Groovy",
        parser_id: Some("groovy"),
        extensions: &["groovy"],
        level: 2,
        level_label: "Tree-sitter bounded body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; JVM runtime is not bundled",
        example: "apps/polyglot-sample/Sample.groovy",
        next_step: "Share JVM-family lowering with Java and Kotlin",
    },
    LanguageSupport {
        language: "JavaScript",
        parser_id: Some("javascript"),
        extensions: &["js", "mjs", "cjs", "jsx"],
        level: 2,
        level_label: "Tree-sitter bounded body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; JS runtime is not bundled",
        example: "apps/polyglot-sample/sample.js",
        next_step: "Add module imports, closures, and JS runtime policy",
    },
    LanguageSupport {
        language: "TypeScript",
        parser_id: Some("typescript"),
        extensions: &["ts", "tsx", "mts", "cts"],
        level: 2,
        level_label: "Tree-sitter bounded body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; TS checker/runtime is not bundled",
        example: "apps/polyglot-sample/sample.ts",
        next_step: "Add module imports, richer statements, and a checker boundary",
    },
    LanguageSupport {
        language: "Kotlin",
        parser_id: Some("kotlin"),
        extensions: &["kt", "kts"],
        level: 2,
        level_label: "Tree-sitter bounded scalar body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; JVM runtime is not bundled",
        example: "apps/polyglot-sample/Sample.kt",
        next_step: "Share JVM-family class metadata, constructors, and runtime strategy",
    },
    LanguageSupport {
        language: "Scala",
        parser_id: Some("scala"),
        extensions: &["scala", "sc"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; JVM runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add parameters, return types, bounded bodies, and JVM runtime strategy",
    },
    LanguageSupport {
        language: "C#",
        parser_id: Some("csharp"),
        extensions: &["cs"],
        level: 2,
        level_label: "Tree-sitter bounded scalar body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; CLR runtime is not bundled",
        example: "apps/polyglot-sample/Program.cs",
        next_step: "Add properties, generics metadata, and CLR runtime strategy",
    },
    LanguageSupport {
        language: "F#",
        parser_id: Some("fsharp"),
        extensions: &["fs", "fsx", "fsi"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; CLR runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add parameters, return types, bounded bodies, and CLR runtime strategy",
    },
    LanguageSupport {
        language: "VB.NET",
        parser_id: Some("vb"),
        extensions: &["vb"],
        level: 0,
        level_label: "known parser id without compatible wired front",
        front: "icore redirect",
        runtime_boundary: "not compiled directly; tools can emit .icore",
        example: "docs/architecture/parser-surface.md",
        next_step: "Wire a grammar or dedicated front before direct source lowering",
    },
    LanguageSupport {
        language: "Python",
        parser_id: Some("python"),
        extensions: &["py", "pyi", "pyw"],
        level: 2,
        level_label: "Tree-sitter bounded scalar body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; Python runtime is not bundled",
        example: "apps/polyglot-sample/sample.py",
        next_step: "Add imports, dynamic object model, and runtime strategy",
    },
    LanguageSupport {
        language: "Ruby",
        parser_id: Some("ruby"),
        extensions: &["rb", "rake", "gemspec"],
        level: 2,
        level_label: "Tree-sitter bounded scalar body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; Ruby runtime is not bundled",
        example: "apps/polyglot-sample/sample.rb",
        next_step: "Add blocks, classes, richer calls, and runtime strategy",
    },
    LanguageSupport {
        language: "PHP",
        parser_id: Some("php"),
        extensions: &["php", "phtml"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; PHP runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add return types, bounded bodies, and runtime strategy",
    },
    LanguageSupport {
        language: "Perl",
        parser_id: Some("perl"),
        extensions: &["pl", "pm"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; Perl runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add parameters, bounded bodies, and runtime strategy",
    },
    LanguageSupport {
        language: "Zig",
        parser_id: Some("zig"),
        extensions: &["zig"],
        level: 2,
        level_label: "Tree-sitter bounded scalar body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; Zig runtime/ABI is not bundled",
        example: "apps/polyglot-sample/sample.zig",
        next_step: "Add comptime-aware boundaries and ABI metadata",
    },
    LanguageSupport {
        language: "Dart",
        parser_id: Some("dart"),
        extensions: &["dart"],
        level: 2,
        level_label: "Tree-sitter bounded scalar body lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; Dart runtime is not bundled",
        example: "apps/polyglot-sample/sample.dart",
        next_step: "Add class metadata, async forms, and runtime policy",
    },
    LanguageSupport {
        language: "Lua",
        parser_id: Some("lua"),
        extensions: &["lua"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; Lua runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add parameters, bounded bodies, and runtime strategy",
    },
    LanguageSupport {
        language: "Clojure",
        parser_id: Some("clojure"),
        extensions: &["clj", "cljs", "cljc"],
        level: 0,
        level_label: "known parser id without compatible wired front",
        front: "icore redirect",
        runtime_boundary: "not compiled directly; tools can emit .icore",
        example: "docs/architecture/parser-surface.md",
        next_step: "Wire a grammar or dedicated front before direct source lowering",
    },
    LanguageSupport {
        language: "Elixir",
        parser_id: Some("elixir"),
        extensions: &["ex", "exs"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; BEAM runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add arity metadata, bounded bodies, and BEAM runtime strategy",
    },
    LanguageSupport {
        language: "Erlang",
        parser_id: Some("erlang"),
        extensions: &["erl", "hrl"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; BEAM runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add arity metadata, bounded bodies, and BEAM runtime strategy",
    },
    LanguageSupport {
        language: "Haskell",
        parser_id: Some("haskell"),
        extensions: &["hs", "lhs"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; Haskell runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add parameters, bounded bodies, and runtime strategy",
    },
    LanguageSupport {
        language: "Nim",
        parser_id: Some("nim"),
        extensions: &["nim"],
        level: 0,
        level_label: "known parser id without compatible wired front",
        front: "icore redirect",
        runtime_boundary: "not compiled directly; tools can emit .icore",
        example: "apps/polyglot-sample/sample.nim",
        next_step: "Wire a grammar or dedicated front before direct source lowering",
    },
    LanguageSupport {
        language: "OCaml",
        parser_id: Some("ocaml"),
        extensions: &["ml", "mli"],
        level: 2,
        level_label: "dedicated bounded let/function lowering",
        front: "compiler::ocaml_front",
        runtime_boundary: "Core IR and textual SIL; OCaml runtime is not bundled",
        example: "apps/polyglot-sample/sample.ml",
        next_step: "Deepen let syntax, pattern matching, modules, and OCaml runtime strategy",
    },
    LanguageSupport {
        language: "Julia",
        parser_id: Some("julia"),
        extensions: &["jl"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; Julia runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add parameters, bounded bodies, and runtime strategy",
    },
    LanguageSupport {
        language: "R",
        parser_id: Some("r"),
        extensions: &["r"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; R runtime is not bundled",
        example: "docs/architecture/parser-surface.md",
        next_step: "Add parameters, bounded bodies, and runtime strategy",
    },
    LanguageSupport {
        language: "D",
        parser_id: Some("d"),
        extensions: &["d"],
        level: 0,
        level_label: "known parser id without compatible wired front",
        front: "icore redirect",
        runtime_boundary: "not compiled directly; tools can emit .icore",
        example: "docs/architecture/parser-surface.md",
        next_step: "Wire a grammar or dedicated front before direct source lowering",
    },
    LanguageSupport {
        language: "Crystal",
        parser_id: Some("crystal"),
        extensions: &["cr"],
        level: 0,
        level_label: "known parser id without compatible wired front",
        front: "icore redirect",
        runtime_boundary: "not compiled directly; tools can emit .icore",
        example: "docs/architecture/parser-surface.md",
        next_step: "Wire a grammar or dedicated front before direct source lowering",
    },
    LanguageSupport {
        language: "Odin",
        parser_id: Some("odin"),
        extensions: &["odin"],
        level: 0,
        level_label: "known parser id without compatible wired front",
        front: "icore redirect",
        runtime_boundary: "not compiled directly; tools can emit .icore",
        example: "apps/polyglot-sample/sample.odin",
        next_step: "Wire a grammar or dedicated front before direct source lowering",
    },
    LanguageSupport {
        language: "Hare",
        parser_id: Some("hare"),
        extensions: &["ha"],
        level: 0,
        level_label: "known parser id without compatible wired front",
        front: "icore redirect",
        runtime_boundary: "not compiled directly; tools can emit .icore",
        example: "apps/polyglot-sample/sample.ha",
        next_step: "Wire a grammar or dedicated front before direct source lowering",
    },
];

#[must_use]
pub fn all_language_support() -> &'static [LanguageSupport] {
    LANGUAGE_SUPPORT
}

#[must_use]
pub fn language_support_for_parser(parser_id: &str) -> Option<&'static LanguageSupport> {
    LANGUAGE_SUPPORT
        .iter()
        .find(|entry| entry.parser_id == Some(parser_id))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser_registry::ParserId;

    #[test]
    fn matrix_tracks_every_parser_id() {
        for parser_id in [
            ParserId::In,
            ParserId::Icore,
            ParserId::C,
            ParserId::Cpp,
            ParserId::ObjC,
            ParserId::ObjCpp,
            ParserId::Java,
            ParserId::Kotlin,
            ParserId::Scala,
            ParserId::CSharp,
            ParserId::FSharp,
            ParserId::VbNet,
            ParserId::Python,
            ParserId::Ruby,
            ParserId::Php,
            ParserId::Perl,
            ParserId::JavaScript,
            ParserId::TypeScript,
            ParserId::Go,
            ParserId::V,
            ParserId::Rust,
            ParserId::Zig,
            ParserId::Dart,
            ParserId::Lua,
            ParserId::Clojure,
            ParserId::Groovy,
            ParserId::Elixir,
            ParserId::Erlang,
            ParserId::Haskell,
            ParserId::OCaml,
            ParserId::Julia,
            ParserId::R,
            ParserId::Nim,
            ParserId::D,
            ParserId::Crystal,
            ParserId::Odin,
            ParserId::Hare,
        ] {
            assert!(
                LANGUAGE_SUPPORT
                    .iter()
                    .any(|entry| entry.parser_id == Some(parser_id.as_str())),
                "missing parser id {}",
                parser_id.as_str()
            );
        }
    }

    #[test]
    fn ruby_reports_bounded_body_lowering() {
        let entry = language_support_for_parser(ParserId::Ruby.as_str()).expect("ruby");
        assert_eq!(entry.level, 2);
        assert!(entry.level_label.contains("body"));
        assert!(entry.runtime_boundary.contains("Core IR"));
    }

    #[test]
    fn level_zero_entries_are_explicit_icore_redirects() {
        for entry in LANGUAGE_SUPPORT.iter().filter(|entry| entry.level == 0) {
            assert!(entry.front.contains("icore"));
            assert!(entry.runtime_boundary.contains(".icore"));
        }
    }

    #[test]
    fn only_in_reports_level_three() {
        let level_three = LANGUAGE_SUPPORT
            .iter()
            .filter(|entry| entry.level == 3)
            .map(|entry| entry.language)
            .collect::<Vec<_>>();
        assert_eq!(level_three, vec!["in"]);
    }

    #[test]
    fn scalar_body_fronts_are_reported_as_level_two() {
        for language in [
            "C",
            "C++",
            "Java",
            "JavaScript",
            "TypeScript",
            "Kotlin",
            "C#",
            "Python",
            "Zig",
            "Dart",
        ] {
            let entry = LANGUAGE_SUPPORT
                .iter()
                .find(|entry| entry.language == language)
                .expect(language);
            assert_eq!(entry.level, 2, "{language}");
            assert!(entry.level_label.contains("body"), "{language}");
            assert!(entry.runtime_boundary.contains("Core IR"), "{language}");
        }
    }

    #[test]
    fn routed_languages_point_at_polyglot_sample_files() {
        for entry in LANGUAGE_SUPPORT.iter().filter(|entry| {
            entry.parser_id.is_some()
                && entry.language != "in"
                && entry.language != "icore"
                && entry.level > 0
                && entry.level <= 2
        }) {
            assert!(
                !entry.example.is_empty(),
                "{} should report an example or documentation surface, got {}",
                entry.language,
                entry.example
            );
        }
    }
}
