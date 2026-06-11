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
        level_label: "bounded Core IR body subset with source diagnostics; closures, try/catch, throw",
        front: "in_lang_parse",
        runtime_boundary: "self-hosted Core IR to textual SIL and bytecode VM subset",
        example: "apps/polyglot-sample/sample.in",
        next_step: "Deepen closure runtime semantics, package binding execution boundaries, and richer class/interface lowering",
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
        level: 3,
        level_label: "dedicated bounded class (impl) and body lowering",
        front: "compiler::rust_front",
        runtime_boundary: "Core IR and textual SIL; rustc is validation only",
        example: "apps/polyglot-sample/sample_class.rs",
        next_step: "Deepen trait support, generics, and remove validation from default success paths",
    },
    LanguageSupport {
        language: "Go",
        parser_id: Some("go"),
        extensions: &["go"],
        level: 2,
        level_label: "dedicated bounded body lowering with struct+method class grouping",
        front: "compiler::go_front",
        runtime_boundary: "Core IR and textual SIL",
        example: "apps/polyglot-sample/sample_class.go",
        next_step: "Deepen declarations, packages, and control flow; add full method body support in class lowering",
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
        level_label: "Tree-sitter bounded scalar body lowering (class/struct lowering pending)",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; standard library/runtime ABI is not bundled",
        example: "apps/polyglot-sample/sample_class.cpp",
        next_step: "Add class_specifier lowering to Decl::Class, methods, templates-as-metadata, and ABI boundaries",
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
        level: 3,
        level_label: "Tree-sitter bounded body lowering with class/interface extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; JVM runtime is not bundled",
        example: "apps/polyglot-sample/Sample.java",
        next_step: "Add constructors, full field type resolution, and JVM runtime strategy",
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
        next_step: "Extract classes, closures, and arrow functions from Tree-sitter CST to Core IR",
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
        next_step: "Extract typed classes, arrow functions, and richer statements from Tree-sitter CST to Core IR",
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
        level: 2,
        level_label: "Tree-sitter bounded body lowering with class/trait extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; JVM runtime is not bundled",
        example: "apps/polyglot-sample/sample.scala",
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
        level: 3,
        level_label: "bounded Function/Sub lowering with family typecheck and optional inline boundary metadata",
        front: "compiler::vb_boundary",
        runtime_boundary: "Core IR and textual SIL; CLR runtime is not bundled",
        example: "apps/polyglot-sample/sample.vb",
        next_step: "Deepen modules, properties, and boundary probe verification",
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
        next_step: "Extract classes, lambdas, and richer bodies from Tree-sitter CST to Core IR",
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
        level: 3,
        level_label: "Tree-sitter bounded body lowering with family typecheck",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; PHP runtime is not bundled",
        example: "apps/polyglot-sample/sample.php",
        next_step: "Add traits, namespaces, and richer class metadata",
    },
    LanguageSupport {
        language: "Perl",
        parser_id: Some("perl"),
        extensions: &["pl", "pm"],
        level: 2,
        level_label: "Tree-sitter bounded body lowering with package/class extraction",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; Perl runtime is not bundled",
        example: "apps/polyglot-sample/sample.pl",
        next_step: "Add parameters, bounded bodies, and runtime strategy",
    },
    LanguageSupport {
        language: "Zig",
        parser_id: Some("zig"),
        extensions: &["zig"],
        level: 3,
        level_label: "Tree-sitter bounded body lowering with family typecheck",
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
        level: 3,
        level_label: "Tree-sitter bounded body lowering with family typecheck",
        front: "compiler::tree_front",
        runtime_boundary: "Core IR and textual SIL; Lua runtime is not bundled",
        example: "apps/polyglot-sample/sample.lua",
        next_step: "Add metatables, richer calls, and runtime strategy",
    },
    LanguageSupport {
        language: "Clojure",
        parser_id: Some("clojure"),
        extensions: &["clj", "cljs", "cljc"],
        level: 3,
        level_label: "bounded defn lowering with family typecheck and optional inline boundary metadata",
        front: "compiler::clojure_boundary",
        runtime_boundary: "Core IR and textual SIL; JVM runtime is not bundled",
        example: "apps/polyglot-sample/sample.clj",
        next_step: "Deepen namespaces, macros, and boundary probe verification",
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
        level: 3,
        level_label: "bounded proc lowering with family typecheck and optional inline boundary metadata",
        front: "compiler::nim_boundary",
        runtime_boundary: "Core IR and textual SIL; Nim runtime is not bundled",
        example: "apps/polyglot-sample/sample.nim",
        next_step: "Deepen modules, types, and boundary probe verification",
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
        level: 3,
        level_label: "bounded function lowering with family typecheck and optional inline boundary metadata",
        front: "compiler::d_boundary",
        runtime_boundary: "Core IR and textual SIL; D runtime is not bundled",
        example: "apps/polyglot-sample/sample.d",
        next_step: "Deepen modules, templates, and boundary probe verification",
    },
    LanguageSupport {
        language: "Crystal",
        parser_id: Some("crystal"),
        extensions: &["cr"],
        level: 3,
        level_label: "bounded def lowering with family typecheck and optional inline boundary metadata",
        front: "compiler::crystal_boundary",
        runtime_boundary: "Core IR and textual SIL; Crystal runtime is not bundled",
        example: "apps/polyglot-sample/sample.cr",
        next_step: "Deepen macros, types, and boundary probe verification",
    },
    LanguageSupport {
        language: "Odin",
        parser_id: Some("odin"),
        extensions: &["odin"],
        level: 3,
        level_label: "bounded proc lowering with family typecheck and optional inline boundary metadata",
        front: "compiler::odin_boundary",
        runtime_boundary: "Core IR and textual SIL; Odin runtime is not bundled",
        example: "apps/polyglot-sample/sample.odin",
        next_step: "Deepen packages, foreign blocks, and boundary probe verification",
    },
    LanguageSupport {
        language: "Hare",
        parser_id: Some("hare"),
        extensions: &["ha"],
        level: 3,
        level_label: "bounded fn lowering with family typecheck and optional inline boundary metadata",
        front: "compiler::hare_boundary",
        runtime_boundary: "Core IR and textual SIL; Hare runtime is not bundled",
        example: "apps/polyglot-sample/sample.ha",
        next_step: "Deepen types, exports, and boundary probe verification",
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
    fn level_three_fronts_include_core_and_promoted_families() {
        let level_three = LANGUAGE_SUPPORT
            .iter()
            .filter(|entry| entry.level == 3)
            .map(|entry| entry.language)
            .collect::<Vec<_>>();
        for language in [
            "in",
            "Rust",
            "Java",
            "PHP",
            "Lua",
            "Zig",
            "Nim",
            "Odin",
            "Hare",
            "D",
            "Crystal",
            "Clojure",
            "VB.NET",
        ] {
            assert!(level_three.contains(&language), "missing {language}");
        }
    }

    #[test]
    fn scalar_body_fronts_are_reported_as_level_two() {
        for language in [
            "C",
            "C++",
            "JavaScript",
            "TypeScript",
            "Kotlin",
            "C#",
            "Python",
            "Dart",
            "Scala",
            "Perl",
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
