//! Textual SIL helpers (inlined for single-crate publish).
//! Source of truth: `compiler/rust-driver/crates/sil`.

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SilArtifact {
    pub function_id: String,
    pub cfg_blocks: Vec<String>,
    pub instructions: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SilAnalysisReport {
    pub call_edges: Vec<(String, String)>,
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
    for line in input.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if line.starts_with("//") {
            continue;
        }
        if let Some(fid) = parse_sil_function_header(line) {
            function_id = fid;
        } else if line.ends_with(':') {
            cfg_blocks.push(line.trim_end_matches(':').to_string());
        } else {
            instructions.push(line.to_string());
        }
    }
    SilArtifact {
        function_id,
        cfg_blocks,
        instructions,
    }
}

pub fn remove_debug_insts(artifact: &SilArtifact) -> SilArtifact {
    let instructions = artifact
        .instructions
        .iter()
        .filter(|inst| !inst.starts_with("debug_value"))
        .cloned()
        .collect();
    SilArtifact {
        function_id: artifact.function_id.clone(),
        cfg_blocks: artifact.cfg_blocks.clone(),
        instructions,
    }
}

pub fn extract_call_graph(artifact: &SilArtifact) -> SilAnalysisReport {
    let caller = artifact.function_id.clone();
    let call_edges = artifact
        .instructions
        .iter()
        .filter_map(|inst| {
            if let Some(rest) = inst.split("function_ref @").nth(1) {
                let callee = rest
                    .split_whitespace()
                    .next()
                    .unwrap_or(rest)
                    .trim()
                    .to_string();
                Some((caller.clone(), callee))
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
}
