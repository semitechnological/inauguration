use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SilArtifact {
    pub function_id: String,
    pub cfg_blocks: Vec<String>,
    pub instructions: Vec<String>,
}

pub fn parse_textual_sil(input: &str) -> SilArtifact {
    let mut function_id = String::from("unknown");
    let mut cfg_blocks = Vec::new();
    let mut instructions = Vec::new();
    for line in input.lines().map(str::trim).filter(|line| !line.is_empty()) {
        if let Some(rest) = line.strip_prefix("sil @") {
            function_id = rest.to_string();
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
}
