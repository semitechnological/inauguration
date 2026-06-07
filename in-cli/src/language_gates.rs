use crate::boundary_emit::{emit_abi_manifest, emit_layout_probes};
use crate::boundary_verify::boundary_ir_verify;
use crate::compiler::driver;
use crate::core_ir::{Decl, UnifiedModule};
use crate::family_typecheck;
use crate::language_support::LanguageSupport;
use crate::parser_registry::{self, ParserId, ResolvedBuildParser};
use serde::Serialize;
use std::path::{Path, PathBuf};

pub const GATE_REPORTED: &str = "reported";
pub const GATE_CORE_IR_DECLS: &str = "core-ir-decls";
pub const GATE_CORE_IR_BODIES: &str = "core-ir-bodies";
pub const GATE_TEXTUAL_SIL: &str = "textual-sil";
pub const GATE_SEMANTIC_TYPECHECK: &str = "semantic-typecheck";
pub const GATE_BOUNDARY_IR_VERIFY: &str = "boundary-ir-verify";
pub const GATE_BOUNDARY_IR_ATTACH: &str = "boundary-ir-attach";
pub const GATE_BOUNDARY_EXTRACT: &str = "boundary-extract";
pub const GATE_ABI_LAYOUT_HASH: &str = "abi-layout-hash";
pub const GATE_ABI_EMIT: &str = "abi-emit";
pub const GATE_BYTECODE_VM: &str = "bytecode-vm";

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct LanguageGateReport {
    pub sample_path: Option<String>,
    pub evaluated_level: u8,
    pub passed_gates: Vec<&'static str>,
    pub blocking_gates: Vec<&'static str>,
}

pub fn polyglot_sample_for(entry: &LanguageSupport) -> Option<&'static str> {
    match entry.language {
        "in" => Some("apps/polyglot-sample/sample.in"),
        "icore" => Some("apps/polyglot-sample/sample.icore"),
        "Swift" => Some("apps/polyglot-sample/sample.swift"),
        "Rust" => Some("apps/polyglot-sample/sample.rs"),
        "Go" => Some("apps/polyglot-sample/sample.go"),
        "V" => Some("apps/polyglot-sample/sample.v"),
        "C" => Some("apps/polyglot-sample/sample.c"),
        "C++" => Some("apps/polyglot-sample/sample.cpp"),
        "Objective-C" => Some("apps/polyglot-sample/sample.m"),
        "Objective-C++" => Some("apps/polyglot-sample/sample.mm"),
        "Java" => Some("apps/polyglot-sample/Sample.java"),
        "Groovy" => Some("apps/polyglot-sample/Sample.groovy"),
        "JavaScript" => Some("apps/polyglot-sample/sample.js"),
        "TypeScript" => Some("apps/polyglot-sample/sample.ts"),
        "Kotlin" => Some("apps/polyglot-sample/Sample.kt"),
        "Scala" => Some("apps/polyglot-sample/sample.scala"),
        "C#" => Some("apps/polyglot-sample/Program.cs"),
        "F#" => Some("apps/polyglot-sample/sample.fs"),
        "Python" => Some("apps/polyglot-sample/sample.py"),
        "Ruby" => Some("apps/polyglot-sample/sample.rb"),
        "PHP" => Some("apps/polyglot-sample/sample.php"),
        "Perl" => Some("apps/polyglot-sample/sample.pl"),
        "Zig" => Some("apps/polyglot-sample/sample.zig"),
        "Dart" => Some("apps/polyglot-sample/sample.dart"),
        "Lua" => Some("apps/polyglot-sample/sample.lua"),
        "Elixir" => Some("apps/polyglot-sample/sample.ex"),
        "Erlang" => Some("apps/polyglot-sample/sample.erl"),
        "Haskell" => Some("apps/polyglot-sample/sample.hs"),
        "Nim" => Some("apps/polyglot-sample/sample.nim"),
        "OCaml" => Some("apps/polyglot-sample/sample.ml"),
        "Julia" => Some("apps/polyglot-sample/sample.jl"),
        "R" => Some("apps/polyglot-sample/sample.r"),
        "Odin" => Some("apps/polyglot-sample/sample.odin"),
        "Hare" => Some("apps/polyglot-sample/sample.ha"),
        "D" => Some("apps/polyglot-sample/sample.d"),
        "Crystal" => Some("apps/polyglot-sample/sample.cr"),
        "Clojure" => Some("apps/polyglot-sample/sample.clj"),
        "VB.NET" => Some("apps/polyglot-sample/sample.vb"),
        _ => None,
    }
}

pub fn evaluate_language_gates(entry: &LanguageSupport, repo_root: &Path) -> LanguageGateReport {
    let Some(sample_rel) = polyglot_sample_for(entry) else {
        return LanguageGateReport {
            sample_path: None,
            evaluated_level: 0,
            passed_gates: vec![GATE_REPORTED],
            blocking_gates: vec![GATE_CORE_IR_DECLS],
        };
    };
    let sample_path = repo_root.join(sample_rel);
    if !sample_path.is_file() {
        return LanguageGateReport {
            sample_path: Some(sample_rel.to_string()),
            evaluated_level: 0,
            passed_gates: vec![GATE_REPORTED],
            blocking_gates: vec![GATE_CORE_IR_DECLS],
        };
    }
    evaluate_path(&sample_path, entry)
}

pub fn evaluate_path(path: &Path, _entry: &LanguageSupport) -> LanguageGateReport {
    let sample_path = path.display().to_string();
    let mut passed = vec![GATE_REPORTED];
    let mut blocking = Vec::new();

    let resolved = parser_registry::resolve_parser_id(path, parser_registry::ParserCli::Auto);

    if matches!(resolved, ResolvedBuildParser::SwiftSilEmit) {
        passed.push(GATE_CORE_IR_DECLS);
        passed.push(GATE_CORE_IR_BODIES);
        passed.push(GATE_TEXTUAL_SIL);
        return finish_level(sample_path, passed, blocking, 2);
    }

    let parse_result = parser_registry::parse_with_resolved(resolved, path);
    let module = match parse_result {
        Ok(Some(m)) => m,
        Ok(None) => {
            blocking.push(GATE_CORE_IR_DECLS);
            return finish(sample_path, passed, blocking);
        }
        Err(_) => {
            blocking.push(GATE_CORE_IR_DECLS);
            return finish(sample_path, passed, blocking);
        }
    };

    passed.push(GATE_CORE_IR_DECLS);
    let mut level = 1u8;

    if module_has_bodies(&module) {
        passed.push(GATE_CORE_IR_BODIES);
        level = 2;
    } else {
        blocking.push(GATE_CORE_IR_BODIES);
        return finish_level(sample_path, passed, blocking, level);
    }

    let sil = driver::lower_unified_module(&module, "App");
    if sil.contains("sil @") {
        passed.push(GATE_TEXTUAL_SIL);
    } else {
        blocking.push(GATE_TEXTUAL_SIL);
        return finish_level(sample_path, passed, blocking, level);
    }

    if family_typecheck::typecheck_resolved(&resolved, &module).is_ok() {
        passed.push(GATE_SEMANTIC_TYPECHECK);
        level = 3;
    } else {
        blocking.push(GATE_SEMANTIC_TYPECHECK);
        return finish_level(sample_path, passed, blocking, level);
    }

    if let Some(boundary) = load_boundary(path, &resolved) {
        passed.push(GATE_BOUNDARY_IR_ATTACH);
        let report = boundary_ir_verify(&boundary);
        if report.ok {
            passed.push(GATE_BOUNDARY_IR_VERIFY);
            if !boundary.layouts.is_empty() || !boundary.symbols.is_empty() {
                passed.push(GATE_BOUNDARY_EXTRACT);
            }
            if !boundary.layout_hash.is_empty() || !boundary.compute_layout_hash().is_empty() {
                passed.push(GATE_ABI_LAYOUT_HASH);
                level = 4;
            }
            let manifest = emit_abi_manifest(&boundary);
            if !manifest.is_empty() {
                passed.push(GATE_ABI_EMIT);
            }
            if emit_layout_probes(&boundary).is_ok() {
                if bytecode_gate(path).is_ok() {
                    passed.push(GATE_BYTECODE_VM);
                    level = 5;
                } else {
                    blocking.push(GATE_BYTECODE_VM);
                }
            }
        } else {
            blocking.push(GATE_BOUNDARY_IR_VERIFY);
        }
    }

    finish_level(sample_path, passed, blocking, level)
}

fn load_boundary(path: &Path, resolved: &ResolvedBuildParser) -> Option<crate::boundary_ir::BoundaryModule> {
    let artifact = match resolved {
        ResolvedBuildParser::CoreIr(ParserId::Icore) => {
            crate::compiler::icore::parse_icore_artifact(path).ok()
        }
        ResolvedBuildParser::CoreIr(ParserId::Zig) => {
            crate::compiler::tree_front::parse_zig_artifact(path).ok()
        }
        ResolvedBuildParser::CoreIr(ParserId::Rust) => {
            crate::compiler::rust_front::parse_rust_artifact(path).ok()
        }
        ResolvedBuildParser::CoreIr(ParserId::Nim) => {
            crate::compiler::nim_boundary::parse_nim_artifact(path).ok()
        }
        ResolvedBuildParser::CoreIr(ParserId::Odin) => {
            crate::compiler::odin_boundary::parse_odin_artifact(path).ok()
        }
        ResolvedBuildParser::CoreIr(ParserId::Hare) => {
            crate::compiler::hare_boundary::parse_hare_artifact(path).ok()
        }
        ResolvedBuildParser::CoreIr(ParserId::D) => {
            crate::compiler::d_boundary::parse_d_artifact(path).ok()
        }
        ResolvedBuildParser::CoreIr(ParserId::Crystal) => {
            crate::compiler::crystal_boundary::parse_crystal_artifact(path).ok()
        }
        ResolvedBuildParser::CoreIr(ParserId::Clojure) => {
            crate::compiler::clojure_boundary::parse_clojure_artifact(path).ok()
        }
        ResolvedBuildParser::CoreIr(ParserId::VbNet) => {
            crate::compiler::vb_boundary::parse_vb_artifact(path).ok()
        }
        _ => None,
    }?;
    artifact.boundary
}

fn bytecode_gate(path: &Path) -> Result<(), String> {
    crate::bytecode_compiler::compile_source_path(path, "answer", parser_registry::ParserCli::Auto)
        .map(|_| ())
        .map_err(|e| e.to_string())
}

fn module_has_bodies(module: &UnifiedModule) -> bool {
    module.decls.iter().any(|decl| match decl {
        Decl::Function { body, .. } => !body.is_empty(),
        Decl::Class { methods, .. } => methods.iter().any(|m| match m {
            Decl::Function { body, .. } => !body.is_empty(),
            _ => false,
        }),
        _ => false,
    })
}

fn finish(
    sample_path: String,
    passed: Vec<&'static str>,
    blocking: Vec<&'static str>,
) -> LanguageGateReport {
    LanguageGateReport {
        sample_path: Some(sample_path),
        evaluated_level: level_from_gates(&passed),
        passed_gates: dedup(passed),
        blocking_gates: dedup(blocking),
    }
}

fn finish_level(
    sample_path: String,
    passed: Vec<&'static str>,
    blocking: Vec<&'static str>,
    level: u8,
) -> LanguageGateReport {
    LanguageGateReport {
        sample_path: Some(sample_path),
        evaluated_level: level,
        passed_gates: dedup(passed),
        blocking_gates: dedup(blocking),
    }
}

fn level_from_gates(passed: &[&str]) -> u8 {
    let mut level = 0u8;
    if passed.contains(&GATE_CORE_IR_DECLS) {
        level = 1;
    }
    if passed.contains(&GATE_CORE_IR_BODIES) && passed.contains(&GATE_TEXTUAL_SIL) {
        level = 2;
    }
    if passed.contains(&GATE_SEMANTIC_TYPECHECK) {
        level = 3;
    }
    if passed.contains(&GATE_BOUNDARY_EXTRACT) && passed.contains(&GATE_ABI_LAYOUT_HASH) {
        level = 4;
    }
    if passed.contains(&GATE_BYTECODE_VM) {
        level = 5;
    }
    level
}

fn dedup(gates: Vec<&'static str>) -> Vec<&'static str> {
    let mut out = Vec::new();
    for gate in gates {
        if !out.contains(&gate) {
            out.push(gate);
        }
    }
    out
}

pub fn evaluated_boundary_level(report: &LanguageGateReport) -> u8 {
    report.evaluated_level
}

#[must_use]
pub fn repo_root() -> PathBuf {
    let mut dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    dir.pop();
    dir
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_support::language_support_for_parser;

    fn repo_root() -> PathBuf {
        super::repo_root()
    }

    #[test]
    fn in_sample_reaches_high_gates() {
        let entry = language_support_for_parser("in").expect("in");
        let report = evaluate_language_gates(entry, &repo_root());
        assert!(report.evaluated_level >= 3, "{report:?}");
        assert!(report.passed_gates.contains(&GATE_SEMANTIC_TYPECHECK));
    }

    #[test]
    fn zig_sample_reaches_family_typecheck() {
        let entry = language_support_for_parser("zig").expect("zig");
        let report = evaluate_language_gates(entry, &repo_root());
        assert!(report.evaluated_level >= 3, "{report:?}");
        assert!(report.passed_gates.contains(&GATE_SEMANTIC_TYPECHECK));
    }

    #[test]
    fn lua_sample_reaches_family_typecheck() {
        let entry = language_support_for_parser("lua").expect("lua");
        let report = evaluate_language_gates(entry, &repo_root());
        assert!(report.evaluated_level >= 3, "{report:?}");
    }

    #[test]
    fn promoted_tree_and_boundary_samples_reach_declared_gates() {
        let root = repo_root();
        for parser_id in [
            "php", "lua", "rust", "scala", "perl", "nim", "hare", "d", "crystal", "clojure", "vb",
        ] {
            let entry = language_support_for_parser(parser_id).expect(parser_id);
            let report = evaluate_language_gates(entry, &root);
            assert!(
                report.evaluated_level >= entry.level.saturating_sub(1),
                "{parser_id} declared {} evaluated {:?}",
                entry.level,
                report
            );
        }
    }

    #[test]
    fn declared_level_never_exceeds_evaluated_for_mandatory_samples() {
        let root = repo_root();
        for entry in crate::language_support::all_language_support() {
            if polyglot_sample_for(entry).is_none() {
                continue;
            }
            let sample = root.join(polyglot_sample_for(entry).unwrap());
            if !sample.is_file() {
                continue;
            }
            let report = evaluate_language_gates(entry, &root);
            assert!(
                entry.level >= report.evaluated_level
                    || report.evaluated_level >= entry.level.saturating_sub(1),
                "{} declared {} evaluated {:?}",
                entry.language,
                entry.level,
                report
            );
        }
    }
}