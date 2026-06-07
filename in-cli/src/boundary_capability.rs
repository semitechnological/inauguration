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
    pub boundary_gates: Vec<&'static str>,
}

#[must_use]
pub fn boundary_level_for(entry: &LanguageSupport) -> u8 {
    match entry.parser_id {
        Some("in") => 5,
        Some("icore") => 3,
        Some("nim" | "odin") => 3,
        Some("zig" | "rust") => 4,
        Some("clojure" | "d" | "crystal" | "hare" | "vb") => 0,
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

#[must_use]
pub fn effective_level_for(entry: &LanguageSupport) -> u8 {
    boundary_level_for(entry).min(entry.level)
}

#[must_use]
pub fn boundary_capability_for(entry: &LanguageSupport) -> BoundaryCapability {
    let boundary_level = boundary_level_for(entry);
    BoundaryCapability {
        boundary_level,
        effective_level: boundary_level.min(entry.level),
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
        boundary_gates: boundary_gates_for(capability.boundary_level),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::language_support::{all_language_support, language_support_for_parser};
    use crate::parser_registry::ParserId;

    #[test]
    fn in_reports_owned_runtime_boundary_level() {
        let entry = language_support_for_parser(ParserId::In.as_str()).expect("in");
        assert_eq!(boundary_level_for(entry), 5);
        assert_eq!(effective_level_for(entry), 3);
        let gates = boundary_gates_for(5);
        assert!(gates.contains(&"bytecode-vm"));
        assert!(gates.contains(&"owned-runtime"));
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
    fn redirect_only_languages_report_boundary_level_zero() {
        for parser_id in [
            ParserId::Clojure,
            ParserId::D,
            ParserId::Crystal,
            ParserId::Hare,
            ParserId::VbNet,
        ] {
            let entry =
                language_support_for_parser(parser_id.as_str()).expect(parser_id.as_str());
            assert_eq!(boundary_level_for(entry), 0, "{}", entry.language);
            assert_eq!(effective_level_for(entry), 0, "{}", entry.language);
            assert_eq!(boundary_gates_for(0), vec!["reported"]);
        }
    }

    #[test]
    fn rust_reports_boundary_extract_level() {
        let entry = language_support_for_parser(ParserId::Rust.as_str()).expect("rust");
        assert_eq!(boundary_level_for(entry), 4);
        assert_eq!(effective_level_for(entry), 3);
        let gates = boundary_gates_for(4);
        assert!(gates.contains(&"boundary-extract"));
        assert!(gates.contains(&"abi-layout-hash"));
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
    fn declaration_only_fronts_report_boundary_level_one() {
        let entry = language_support_for_parser(ParserId::Scala.as_str()).expect("scala");
        assert_eq!(boundary_level_for(entry), 1);
        assert_eq!(effective_level_for(entry), 1);
        let gates = boundary_gates_for(1);
        assert_eq!(gates, vec!["reported", "core-ir-decls"]);
    }

    #[test]
    fn language_support_json_includes_boundary_fields() {
        let entry = all_language_support()
            .iter()
            .find(|entry| entry.language == "in")
            .expect("in");
        let json = language_support_json(entry);
        assert_eq!(json.boundary_level, 5);
        assert_eq!(json.effective_level, 3);
        assert!(json.boundary_gates.contains(&"owned-runtime"));
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