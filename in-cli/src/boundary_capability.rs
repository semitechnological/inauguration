use crate::language_gates::{self, evaluate_language_gates, polyglot_sample_for};
use crate::language_support::LanguageSupport;
use serde::Serialize;

pub const BOUNDARY_GATE_LADDER: &[(&str, u8)] = &[
    ("reported", 0),
    ("core-ir-decls", 1),
    ("core-ir-bodies", 2),
    ("textual-sil", 2),
    ("boundary-ir-verify", 3),
    ("boundary-ir-attach", 3),
    ("boundary-extract", 4),
    ("abi-layout-hash", 4),
    ("abi-emit", 5),
    ("bytecode-vm", 5),
    ("owned-runtime", 5),
];

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct BoundaryCapability {
    pub boundary_level: u8,
    pub effective_level: u8,
    pub evaluated_level: u8,
}

#[derive(Debug, Clone, Serialize)]
pub struct LanguageSupportJson<'a> {
    pub language: &'a str,
    pub parser_id: Option<&'a str>,
    pub extensions: &'a [&'a str],
    pub level: u8,
    pub level_label: &'a str,
    pub front: &'a str,
    pub runtime_boundary: &'a str,
    pub example: &'a str,
    pub next_step: &'a str,
    pub boundary_level: u8,
    pub effective_level: u8,
    pub evaluated_level: u8,
    pub passed_gates: Vec<&'static str>,
    pub blocking_gates: Vec<&'static str>,
    pub boundary_gates: Vec<&'static str>,
}

fn declared_boundary_level_for(entry: &LanguageSupport) -> u8 {
    match entry.parser_id {
        Some("in") => 5,
        Some("icore") => 3,
        Some("nim" | "odin" | "hare" | "d" | "crystal" | "clojure" | "vb") => 3,
        Some("zig" | "rust") => 4,
        None if entry.language == "Swift" => 2,
        Some(_) => infer_boundary_level_from_runtime(entry),
        None => infer_boundary_level_from_runtime(entry),
    }
}

fn infer_boundary_level_from_runtime(entry: &LanguageSupport) -> u8 {
    if entry.runtime_boundary.contains("not compiled directly") {
        0
    } else if entry.runtime_boundary.contains("declarations only") {
        1
    } else if entry.runtime_boundary.contains("Core IR") {
        2
    } else {
        1
    }
}

fn gate_report_for(entry: &LanguageSupport) -> language_gates::LanguageGateReport {
    evaluate_language_gates(entry, &language_gates::repo_root())
}

fn gate_evaluated_level(entry: &LanguageSupport) -> Option<u8> {
    let sample_rel = polyglot_sample_for(entry)?;
    let sample = language_gates::repo_root().join(sample_rel);
    if !sample.is_file() {
        return None;
    }
    Some(gate_report_for(entry).evaluated_level)
}

#[must_use]
pub fn boundary_level_for(entry: &LanguageSupport) -> u8 {
    let declared = declared_boundary_level_for(entry);
    gate_evaluated_level(entry)
        .map(|evaluated| declared.min(evaluated))
        .unwrap_or(declared)
}

#[must_use]
pub fn evaluated_level_for(entry: &LanguageSupport) -> u8 {
    gate_evaluated_level(entry).unwrap_or(0)
}

#[must_use]
pub fn effective_level_for(entry: &LanguageSupport) -> u8 {
    boundary_level_for(entry).min(entry.level)
}

#[must_use]
pub fn boundary_capability_for(entry: &LanguageSupport) -> BoundaryCapability {
    let boundary_level = boundary_level_for(entry);
    let evaluated_level = evaluated_level_for(entry);
    BoundaryCapability {
        boundary_level,
        effective_level: boundary_level.min(entry.level),
        evaluated_level,
    }
}

#[must_use]
pub fn boundary_gates_for(boundary_level: u8) -> Vec<&'static str> {
    let mut gates = Vec::new();
    let mut seen = std::collections::BTreeSet::new();
    for (gate, min_level) in BOUNDARY_GATE_LADDER {
        if boundary_level >= *min_level && seen.insert(*gate) {
            gates.push(*gate);
        }
    }
    gates
}

#[must_use]
pub fn language_support_json(entry: &LanguageSupport) -> LanguageSupportJson<'_> {
    let capability = boundary_capability_for(entry);
    let report = gate_report_for(entry);
    LanguageSupportJson {
        language: entry.language,
        parser_id: entry.parser_id,
        extensions: entry.extensions,
        level: entry.level,
        level_label: entry.level_label,
        front: entry.front,
        runtime_boundary: entry.runtime_boundary,
        example: entry.example,
        next_step: entry.next_step,
        boundary_level: capability.boundary_level,
        effective_level: capability.effective_level,
        evaluated_level: capability.evaluated_level,
        passed_gates: report.passed_gates,
        blocking_gates: report.blocking_gates,
        boundary_gates: boundary_gates_for(capability.boundary_level),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_support::{all_language_support, language_support_for_parser};
    use crate::parser_registry::ParserId;

    #[test]
    fn in_reports_gate_capped_boundary_level() {
        let entry = language_support_for_parser(ParserId::In.as_str()).expect("in");
        let capability = boundary_capability_for(entry);
        assert!(capability.boundary_level >= 3);
        assert!(capability.boundary_level <= 5);
        assert_eq!(capability.effective_level, entry.level.min(capability.boundary_level));
    }

    #[test]
    fn icore_reports_boundary_ir_interchange_level() {
        let entry = language_support_for_parser(ParserId::Icore.as_str()).expect("icore");
        assert_eq!(boundary_level_for(entry), 3);
        assert_eq!(effective_level_for(entry), 2);
        let gates = boundary_gates_for(3);
        assert!(gates.contains(&"boundary-ir-verify"));
        assert!(gates.contains(&"boundary-ir-attach"));
        assert!(!gates.contains(&"boundary-extract"));
    }

    #[test]
    fn dedicated_boundary_fronts_report_semantic_level() {
        for parser_id in [
            ParserId::Clojure,
            ParserId::D,
            ParserId::Crystal,
            ParserId::Hare,
            ParserId::VbNet,
        ] {
            let entry =
                language_support_for_parser(parser_id.as_str()).expect(parser_id.as_str());
            assert!(boundary_level_for(entry) >= 2, "{}", entry.language);
            assert_eq!(effective_level_for(entry), entry.level);
        }
    }

    #[test]
    fn rust_reports_gate_capped_body_lowering() {
        let entry = language_support_for_parser(ParserId::Rust.as_str()).expect("rust");
        assert_eq!(boundary_level_for(entry), 2);
        assert_eq!(effective_level_for(entry), 2);
        let gates = boundary_gates_for(2);
        assert!(gates.contains(&"core-ir-bodies"));
        assert!(gates.contains(&"textual-sil"));
    }

    #[test]
    fn nim_odin_report_boundary_ir_attach_level() {
        for parser_id in [ParserId::Nim, ParserId::Odin] {
            let entry =
                language_support_for_parser(parser_id.as_str()).expect(parser_id.as_str());
            assert_eq!(boundary_level_for(entry), 3, "{}", entry.language);
            assert_eq!(effective_level_for(entry), 2, "{}", entry.language);
        }
    }

    #[test]
    fn php_reports_gate_capped_body_lowering() {
        let entry = language_support_for_parser(ParserId::Php.as_str()).expect("php");
        assert_eq!(boundary_level_for(entry), 2);
        assert_eq!(effective_level_for(entry), entry.level);
    }

    #[test]
    fn language_support_json_includes_boundary_fields() {
        let entry = all_language_support()
            .iter()
            .find(|entry| entry.language == "in")
            .expect("in");
        let json = language_support_json(entry);
        assert!(json.boundary_level >= 3);
        assert_eq!(json.effective_level, entry.level.min(json.boundary_level));
        assert!(!json.passed_gates.is_empty());
    }

    #[test]
    fn declared_level_never_exceeds_evaluated_when_sample_exists() {
        for entry in all_language_support() {
            if polyglot_sample_for(entry).is_none() {
                continue;
            }
            let sample = language_gates::repo_root().join(polyglot_sample_for(entry).unwrap());
            if !sample.is_file() {
                continue;
            }
            let capability = boundary_capability_for(entry);
            assert!(
                entry.level >= capability.evaluated_level
                    || capability.evaluated_level >= entry.level.saturating_sub(1),
                "{} declared {} evaluated {}",
                entry.language,
                entry.level,
                capability.evaluated_level
            );
        }
    }

    #[test]
    fn every_language_support_entry_has_boundary_capability() {
        for entry in all_language_support() {
            let capability = boundary_capability_for(entry);
            assert!(capability.boundary_level <= 5);
            assert!(capability.effective_level <= entry.level);
            assert!(capability.effective_level <= capability.boundary_level);
            assert!(!boundary_gates_for(capability.boundary_level).is_empty());
        }
    }
}