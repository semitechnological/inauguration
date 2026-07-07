//! Textual SIL helpers (inlined for single-crate publish).
//! Source of truth: `compiler/rust-driver/crates/sil`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SilFunctionRecord {
    pub function_id: String,
    pub cfg_blocks: Vec<String>,
    pub instructions: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SilArtifact {
    /// Last `sil @…` header in the merged blob (emitters often place `@main` last).
    pub function_id: String,
    pub cfg_blocks: Vec<String>,
    pub instructions: Vec<String>,
    /// Parallel to `instructions`: which `sil @…` scope each instruction line was parsed under.
    /// Empty means unknown / legacy deserialize — [`extract_call_graph`] falls back to `function_id` for every instruction.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub instruction_callers: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub functions: Vec<SilFunctionRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SilAnalysisReport {
    pub call_edges: Vec<(String, String)>,
}

impl SilAnalysisReport {
    pub fn callers_of(&self, callee: &str) -> Vec<String> {
        self.call_edges
            .iter()
            .filter(|(_, c)| c == callee)
            .map(|(caller, _)| caller.clone())
            .collect()
    }
}

fn parse_sil_function_header(line: &str) -> Option<String> {
    if !line.starts_with("sil ") {
        return None;
    }
    let rest = line.strip_prefix("sil ")?;
    let at = rest.find('@')?;
    let tail = &rest[at + 1..];
    let name = tail
        .split(|c: char| c.is_whitespace() || c == '(')
        .next()
        .unwrap_or("")
        .trim_end_matches(':')
        .to_string();
    if name.is_empty() { None } else { Some(name) }
}

pub fn parse_textual_sil(input: &str) -> SilArtifact {
    let mut function_id = String::from("unknown");
    let mut cfg_blocks = Vec::new();
    let mut instructions = Vec::new();
    let mut instruction_callers = Vec::new();
    let mut functions = Vec::new();
    let mut current_function: Option<SilFunctionRecord> = None;
    for line in input.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.starts_with("//") {
            continue;
        }
        if let Some(fid) = parse_sil_function_header(line) {
            function_id = fid;
            if let Some(record) = current_function.take() {
                functions.push(record);
            }
            current_function = Some(SilFunctionRecord {
                function_id: function_id.clone(),
                cfg_blocks: Vec::new(),
                instructions: Vec::new(),
            });
        } else if line.ends_with(':') {
            let block = line.trim_end_matches(':').to_string();
            cfg_blocks.push(block.clone());
            current_function
                .get_or_insert_with(|| SilFunctionRecord {
                    function_id: function_id.clone(),
                    cfg_blocks: Vec::new(),
                    instructions: Vec::new(),
                })
                .cfg_blocks
                .push(block);
        } else {
            let instruction = line.to_string();
            instructions.push(instruction.clone());
            instruction_callers.push(function_id.clone());
            current_function
                .get_or_insert_with(|| SilFunctionRecord {
                    function_id: function_id.clone(),
                    cfg_blocks: Vec::new(),
                    instructions: Vec::new(),
                })
                .instructions
                .push(instruction);
        }
    }
    if let Some(record) = current_function {
        functions.push(record);
    }
    SilArtifact {
        function_id,
        cfg_blocks,
        instructions,
        instruction_callers,
        functions,
    }
}

pub fn remove_debug_insts(artifact: &SilArtifact) -> SilArtifact {
    let paired: Vec<_> = if artifact.instruction_callers.len() == artifact.instructions.len() {
        artifact
            .instructions
            .iter()
            .zip(artifact.instruction_callers.iter())
            .filter(|(inst, _)| !inst.starts_with("debug_value"))
            .map(|(i, c)| (i.clone(), c.clone()))
            .collect()
    } else {
        artifact
            .instructions
            .iter()
            .filter(|inst| !inst.starts_with("debug_value"))
            .map(|i| (i.clone(), artifact.function_id.clone()))
            .collect()
    };
    let (instructions, instruction_callers): (Vec<_>, Vec<_>) = paired.into_iter().unzip();
    let functions = artifact
        .functions
        .iter()
        .map(|function| SilFunctionRecord {
            function_id: function.function_id.clone(),
            cfg_blocks: function.cfg_blocks.clone(),
            instructions: function
                .instructions
                .iter()
                .filter(|inst| !inst.starts_with("debug_value"))
                .cloned()
                .collect(),
        })
        .collect();
    SilArtifact {
        function_id: artifact.function_id.clone(),
        cfg_blocks: artifact.cfg_blocks.clone(),
        instructions,
        instruction_callers,
        functions,
    }
}

pub fn extract_call_graph(artifact: &SilArtifact) -> SilAnalysisReport {
    if !artifact.functions.is_empty() {
        let call_edges = artifact
            .functions
            .iter()
            .flat_map(|function| {
                function.instructions.iter().filter_map(|inst| {
                    if let Some(rest) = inst.split("function_ref @").nth(1) {
                        let callee = rest
                            .split_whitespace()
                            .next()
                            .unwrap_or(rest)
                            .trim()
                            .to_string();
                        Some((function.function_id.clone(), callee))
                    } else {
                        None
                    }
                })
            })
            .collect();
        return SilAnalysisReport { call_edges };
    }
    let fallback_caller = artifact.function_id.clone();
    let use_per_inst_callers = artifact.instruction_callers.len() == artifact.instructions.len()
        && !artifact.instruction_callers.is_empty();
    let call_edges = artifact
        .instructions
        .iter()
        .enumerate()
        .filter_map(|(idx, inst)| {
            let caller = if use_per_inst_callers {
                artifact.instruction_callers[idx].clone()
            } else {
                fallback_caller.clone()
            };
            if let Some(rest) = inst.split("function_ref @").nth(1) {
                let callee = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or(rest)
                    .trim()
                    .to_string();
                Some((caller, callee))
            } else {
                None
            }
        })
        .collect();
    SilAnalysisReport { call_edges }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_function_name_and_blocks() {
        let artifact = parse_textual_sil(
            "sil @my_function\nentry:\n%0 = integer_literal $Builtin.Int64, 1\nexit:",
        );
        assert_eq!(artifact.function_id, "my_function");
        assert_eq!(artifact.cfg_blocks, vec!["entry", "exit"]);
    }

    #[test]
    fn strips_debug_value_lines() {
        let artifact = parse_textual_sil(
            "sil @main\nentry:\ndebug_value %0\n%1 = integer_literal $Builtin.Int64, 1",
        );
        let reduced = remove_debug_insts(&artifact);
        assert_eq!(reduced.instructions.len(), 1);
    }

    #[test]
    fn extracts_call_graph_edges_from_function_ref() {
        let artifact =
            parse_textual_sil("sil @main\nentry:\n%0 = function_ref @helper : $@convention(thin)");
        let report = extract_call_graph(&artifact);
        assert_eq!(
            report.call_edges,
            vec![("main".to_string(), "helper".to_string())]
        );
    }

    #[test]
    fn merged_multi_function_call_graph_uses_per_instruction_caller() {
        let sil = concat!(
            "sil @helper\nbb0:\n",
            "%0 = function_ref @deep : $@convention(thin)\n",
            "sil @main\nbb0:\n",
            "%1 = function_ref @helper : $@convention(thin)\n",
        );
        let artifact = parse_textual_sil(sil);
        assert_eq!(artifact.function_id, "main");
        assert_eq!(
            artifact.instruction_callers,
            vec!["helper".to_string(), "main".to_string()]
        );
        let report = extract_call_graph(&artifact);
        assert_eq!(
            report.call_edges,
            vec![
                ("helper".to_string(), "deep".to_string()),
                ("main".to_string(), "helper".to_string()),
            ]
        );
    }

    #[test]
    fn extract_call_graph_legacy_mismatched_callers_falls_back_to_function_id_edge_case() {
        let artifact = SilArtifact {
            function_id: "main".into(),
            cfg_blocks: vec![],
            instructions: vec![
                "%0 = function_ref @a : $@convention(thin)".into(),
                "%1 = function_ref @b : $@convention(thin)".into(),
            ],
            instruction_callers: vec!["caller_a".into()],
            functions: vec![],
        };
        let report = extract_call_graph(&artifact);
        assert_eq!(
            report.call_edges,
            vec![
                ("main".to_string(), "a".to_string()),
                ("main".to_string(), "b".to_string()),
            ]
        );
    }

    #[test]
    fn extract_call_graph_uses_functions_array() {
        let artifact = SilArtifact {
            function_id: "main".into(),
            cfg_blocks: vec![],
            instructions: vec![],
            instruction_callers: vec![],
            functions: vec![
                SilFunctionRecord {
                    function_id: "func1".into(),
                    cfg_blocks: vec![],
                    instructions: vec!["%0 = function_ref @callee1".into()],
                },
                SilFunctionRecord {
                    function_id: "func2".into(),
                    cfg_blocks: vec![],
                    instructions: vec!["%1 = function_ref @callee2 : $@convention(thin)".into()],
                },
            ],
        };
        let report = extract_call_graph(&artifact);
        assert_eq!(
            report.call_edges,
            vec![
                ("func1".to_string(), "callee1".to_string()),
                ("func2".to_string(), "callee2".to_string()),
            ]
        );
    }

    #[test]
    fn extract_call_graph_handles_no_whitespace_after_callee() {
        let artifact = SilArtifact {
            function_id: "main".into(),
            cfg_blocks: vec![],
            instructions: vec!["%0 = function_ref @callee".into()],
            instruction_callers: vec![],
            functions: vec![],
        };
        let report = extract_call_graph(&artifact);
        assert_eq!(
            report.call_edges,
            vec![("main".to_string(), "callee".to_string())]
        );
    }

    #[test]
    fn extract_call_graph_legacy_uses_instruction_callers_when_lengths_match() {
        let artifact = SilArtifact {
            function_id: "main".into(),
            cfg_blocks: vec![],
            instructions: vec![
                "%0 = function_ref @a : $@convention(thin)".into(),
                "%1 = function_ref @b : $@convention(thin)".into(),
            ],
            instruction_callers: vec!["caller_a".into(), "caller_b".into()],
            functions: vec![],
        };
        let report = extract_call_graph(&artifact);
        assert_eq!(
            report.call_edges,
            vec![
                ("caller_a".to_string(), "a".to_string()),
                ("caller_b".to_string(), "b".to_string()),
            ]
        );
    }

    #[test]
    fn extract_call_graph_legacy_missing_instruction_callers_falls_back_to_function_id() {
        let artifact = SilArtifact {
            function_id: "main".into(),
            cfg_blocks: vec![],
            instructions: vec![
                "%0 = function_ref @a : $@convention(thin)".into(),
                "%1 = function_ref @b : $@convention(thin)".into(),
            ],
            instruction_callers: vec![],
            functions: vec![],
        };
        let report = extract_call_graph(&artifact);
        assert_eq!(
            report.call_edges,
            vec![
                ("main".to_string(), "a".to_string()),
                ("main".to_string(), "b".to_string()),
            ]
        );
    }

    #[test]
    fn remove_debug_insts_keeps_instruction_callers_aligned() {
        let artifact = parse_textual_sil(
            "sil @helper\nentry:\ndebug_value %x\n%0 = function_ref @x : $@convention(thin)\nsil @main\nbb0:\n%1 = integer_literal $Builtin.Int64, 1\n",
        );
        let cleaned = remove_debug_insts(&artifact);
        assert_eq!(cleaned.instructions.len(), 2);
        assert_eq!(
            cleaned.instruction_callers,
            vec!["helper".to_string(), "main".to_string()]
        );
        assert_eq!(
            extract_call_graph(&cleaned).call_edges,
            vec![("helper".to_string(), "x".to_string())]
        );
    }

    #[test]
    fn remove_debug_insts_with_instruction_callers_removes_debug() {
        let artifact = SilArtifact {
            function_id: "main".into(),
            cfg_blocks: vec!["bb0".into()],
            instructions: vec![
                "debug_value %0".into(),
                "%1 = function_ref @a : $@convention(thin)".into(),
                "debug_value %2".into(),
            ],
            instruction_callers: vec!["main".into(), "main".into(), "main".into()],
            functions: vec![],
        };
        let cleaned = remove_debug_insts(&artifact);
        assert_eq!(cleaned.instructions.len(), 1);
        assert_eq!(
            cleaned.instructions[0],
            "%1 = function_ref @a : $@convention(thin)"
        );
        assert_eq!(cleaned.instruction_callers.len(), 1);
        assert_eq!(cleaned.instruction_callers[0], "main");
    }

    #[test]
    fn remove_debug_insts_legacy_missing_instruction_callers_removes_debug() {
        let artifact = SilArtifact {
            function_id: "main".into(),
            cfg_blocks: vec!["bb0".into()],
            instructions: vec![
                "debug_value %0".into(),
                "%1 = function_ref @a : $@convention(thin)".into(),
                "debug_value %2".into(),
            ],
            instruction_callers: vec![],
            functions: vec![],
        };
        let cleaned = remove_debug_insts(&artifact);
        assert_eq!(cleaned.instructions.len(), 1);
        assert_eq!(
            cleaned.instructions[0],
            "%1 = function_ref @a : $@convention(thin)"
        );
        assert_eq!(cleaned.instruction_callers.len(), 1);
        assert_eq!(cleaned.instruction_callers[0], "main");
    }

    #[test]
    fn merged_multi_function_sil_records_each_function() {
        let artifact = parse_textual_sil(concat!(
            "sil @helper\nbb0:\n",
            "%0 = function_ref @deep : $@convention(thin)\n",
            "sil @main\nbb0:\n",
            "%1 = function_ref @helper : $@convention(thin)\n",
        ));
        assert_eq!(artifact.function_id, "main");
        assert_eq!(artifact.instructions.len(), 2);
        assert_eq!(artifact.functions.len(), 2);
        assert_eq!(artifact.functions[0].function_id, "helper");
        assert_eq!(artifact.functions[0].cfg_blocks, vec!["bb0"]);
        assert_eq!(
            artifact.functions[0].instructions,
            vec!["%0 = function_ref @deep : $@convention(thin)"]
        );
        assert_eq!(artifact.functions[1].function_id, "main");
        assert_eq!(artifact.functions[1].cfg_blocks, vec!["bb0"]);
        assert_eq!(
            artifact.functions[1].instructions,
            vec!["%1 = function_ref @helper : $@convention(thin)"]
        );
    }

    #[test]
    fn remove_debug_insts_mismatched_instruction_callers_removes_debug() {
        let artifact = SilArtifact {
            function_id: "fallback_func".into(),
            cfg_blocks: vec![],
            instructions: vec![
                "debug_value %0".into(),
                "%1 = function_ref @a : $@convention(thin)".into(),
                "debug_value %2".into(),
            ],
            instruction_callers: vec!["some_caller".into()],
            functions: vec![],
        };
        let cleaned = remove_debug_insts(&artifact);
        assert_eq!(cleaned.instructions.len(), 1);
        assert_eq!(
            cleaned.instructions[0],
            "%1 = function_ref @a : $@convention(thin)"
        );
        assert_eq!(cleaned.instruction_callers.len(), 1);
        assert_eq!(cleaned.instruction_callers[0], "fallback_func");
    }

    #[test]
    fn remove_debug_insts_with_only_debug_values_returns_empty() {
        let artifact = SilArtifact {
            function_id: "main".into(),
            cfg_blocks: vec![],
            instructions: vec!["debug_value %0".into(), "debug_value %1".into()],
            instruction_callers: vec!["main".into(), "main".into()],
            functions: vec![],
        };
        let cleaned = remove_debug_insts(&artifact);
        assert!(cleaned.instructions.is_empty());
        assert!(cleaned.instruction_callers.is_empty());
    }

    #[test]
    fn remove_debug_insts_keeps_function_records_aligned() {
        let artifact = parse_textual_sil(concat!(
            "sil @helper\nentry:\n",
            "debug_value %x\n",
            "%0 = function_ref @x : $@convention(thin)\n",
            "sil @main\nbb0:\n",
            "debug_value %y\n",
            "%1 = integer_literal $Builtin.Int64, 1\n",
        ));
        let cleaned = remove_debug_insts(&artifact);
        assert_eq!(cleaned.functions.len(), 2);
        assert_eq!(
            cleaned.functions[0].instructions,
            vec!["%0 = function_ref @x : $@convention(thin)"]
        );
        assert_eq!(
            cleaned.functions[1].instructions,
            vec!["%1 = integer_literal $Builtin.Int64, 1"]
        );
    }
}
