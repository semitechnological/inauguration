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
        level: 2,
        level_label: "Core IR body subset",
        front: "in_lang_parse",
        runtime_boundary: "self-hosted Core IR to textual SIL and bytecode VM subset",
        example: "apps/polyglot-sample/sample.in",
        next_step: "Deepen diagnostics, type rules, and executable runtime coverage",
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
        level_label: "Tree-sitter trivial return lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; libc/runtime ABI is not bundled",
        example: "apps/polyglot-sample/sample.c",
        next_step: "Add locals, calls, pointer types, and ABI boundaries",
    },
    LanguageSupport {
        language: "C++",
        parser_id: Some("cpp"),
        extensions: &["cc", "cpp", "cxx", "hpp", "hxx", "hh", "h++", "ipp"],
        level: 2,
        level_label: "Tree-sitter trivial return lowering",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; standard library/runtime ABI is not bundled",
        example: "apps/polyglot-sample/sample.cpp",
        next_step: "Add namespaces, methods, calls, templates-as-metadata, and ABI boundaries",
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
        next_step: "Add typed parameters, richer statements, and a checker boundary",
    },
    LanguageSupport {
        language: "Kotlin",
        parser_id: Some("kotlin"),
        extensions: &["kt", "kts"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; JVM runtime is not bundled",
        example: "apps/polyglot-sample/Sample.kt",
        next_step: "Lift Java/Groovy body lowering into Kotlin",
    },
    LanguageSupport {
        language: "C#",
        parser_id: Some("csharp"),
        extensions: &["cs"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; CLR runtime is not bundled",
        example: "apps/polyglot-sample/Program.cs",
        next_step: "Add method bodies, properties, generics metadata, and CLR runtime strategy",
    },
    LanguageSupport {
        language: "Python",
        parser_id: Some("python"),
        extensions: &["py", "pyi", "pyw"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; Python runtime is not bundled",
        example: "apps/polyglot-sample/sample.py",
        next_step: "Add def bodies, imports, dynamic object model, and runtime strategy",
    },
    LanguageSupport {
        language: "Ruby",
        parser_id: Some("ruby"),
        extensions: &["rb", "rake", "gemspec"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; Ruby runtime is not bundled",
        example: "apps/polyglot-sample/sample.rb",
        next_step: "Add method bodies, blocks, classes, and runtime strategy",
    },
    LanguageSupport {
        language: "Zig",
        parser_id: Some("zig"),
        extensions: &["zig"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; Zig runtime/ABI is not bundled",
        example: "apps/polyglot-sample/sample.zig",
        next_step: "Add comptime-aware boundaries and bounded function bodies",
    },
    LanguageSupport {
        language: "Dart",
        parser_id: Some("dart"),
        extensions: &["dart"],
        level: 1,
        level_label: "Tree-sitter declaration extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR declarations only; Dart runtime is not bundled",
        example: "apps/polyglot-sample/sample.dart",
        next_step: "Add class/function bodies and runtime policy",
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
        level: 0,
        level_label: "known parser id without compatible wired front",
        front: "icore redirect",
        runtime_boundary: "not compiled directly; tools can emit .icore",
        example: "apps/polyglot-sample/sample.ml",
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

    #[test]
    fn matrix_tracks_requested_language_surface() {
        for language in [
            "V",
            "Go",
            "Rust",
            "OCaml",
            "TypeScript",
            "JavaScript",
            "Swift",
            "Dart",
            "Python",
            "Java",
            "Kotlin",
            "C++",
            "C#",
            "C",
            "Zig",
            "Nim",
            "Odin",
            "Hare",
            "Ruby",
        ] {
            assert!(
                LANGUAGE_SUPPORT
                    .iter()
                    .any(|entry| entry.language.eq_ignore_ascii_case(language)),
                "missing {language}"
            );
        }
    }

    #[test]
    fn level_zero_entries_are_explicit_icore_redirects() {
        for entry in LANGUAGE_SUPPORT.iter().filter(|entry| entry.level == 0) {
            assert!(entry.front.contains("icore"));
            assert!(entry.runtime_boundary.contains(".icore"));
        }
    }

    #[test]
    fn routed_languages_point_at_polyglot_sample_files() {
        for entry in LANGUAGE_SUPPORT.iter().filter(|entry| {
            entry.parser_id.is_some()
                && entry.language != "in"
                && entry.language != "icore"
                && entry.level <= 2
        }) {
            assert!(
                entry.example.starts_with("apps/polyglot-sample/"),
                "{} should use a checked-in polyglot sample, got {}",
                entry.language,
                entry.example
            );
        }
    }
}
