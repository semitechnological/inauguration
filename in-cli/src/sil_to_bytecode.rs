//! Lower SIL to bytecode for execution or JIT compilation.

use crate::bytecode::{BytecodeFunction, BytecodeModule, Instruction};
use crate::hybrid_sil::SilArtifact;
use std::collections::HashMap;

/// Lower a SIL artifact to bytecode.
pub fn lower_sil_to_bytecode(artifact: &SilArtifact) -> Result<BytecodeModule, String> {
    let mut module = BytecodeModule::new(artifact.function_id.clone());

    // For now, emit a single-function module with simple lowering.
    // In a full compiler, we'd parse each `sil @name` block as a separate function.

    let mut instructions = Vec::new();
    let mut local_counter = 0;
    let mut value_map: HashMap<String, usize> = HashMap::new();
    let mut label_counter = 0;

    for line in &artifact.instructions {
        let line = line.trim();

        // Parse SIL instructions and convert to bytecode
        if let Ok(inst) = parse_sil_instruction_to_bytecode(
            line,
            &mut local_counter,
            &mut value_map,
            &mut label_counter,
        ) {
            instructions.extend(inst);
        }
    }

    // Always end with return
    if !matches!(instructions.last(), Some(Instruction::Return)) {
        instructions.push(Instruction::Return);
    }

    let func = BytecodeFunction {
        name: artifact.function_id.clone(),
        instructions,
        local_count: local_counter,
    };

    module.add_function(func);
    Ok(module)
}

/// Parse a single SIL instruction and emit bytecode equivalent(s).
fn parse_sil_instruction_to_bytecode(
    line: &str,
    local_counter: &mut usize,
    value_map: &mut HashMap<String, usize>,
    _label_counter: &mut usize,
) -> Result<Vec<Instruction>, String> {
    let mut out = Vec::new();
    let line = line.trim();

    // integer_literal $Builtin.Int64, 42 → LoadInt(42)
    if let Some(rest) = line.strip_prefix("integer_literal $Builtin.Int64,") {
        if let Ok(n) = rest.trim().parse::<i64>() {
            out.push(Instruction::LoadInt(n));
            return Ok(out);
        }
    }

    // %0 = integer_literal $Builtin.Int64, 42
    if let Some(before_eq) = line.split('=').next() {
        let reg = before_eq.trim();
        if reg.starts_with('%') {
            if let Some(rest) = line.split('=').nth(1) {
                let rest = rest.trim();
                if let Some(n_str) = rest.strip_prefix("integer_literal $Builtin.Int64,") {
                    if let Ok(n) = n_str.trim().parse::<i64>() {
                        out.push(Instruction::LoadInt(n));
                        // Store in "register" (local)
                        let slot = *local_counter;
                        *local_counter += 1;
                        value_map.insert(reg.to_string(), slot);
                        out.push(Instruction::Store(slot));
                        return Ok(out);
                    }
                }
            }
        }
    }

    // function_ref @helper → emit a reference (stub for now)
    if let Some(rest) = line.strip_prefix("function_ref @") {
        let func_name = rest.split_whitespace().next().unwrap_or("?");
        out.push(Instruction::LoadString(func_name.to_string()));
        return Ok(out);
    }

    // return → Return
    if line.starts_with("return") {
        out.push(Instruction::Return);
        return Ok(out);
    }

    // bb0: / bb1: → Label
    if line.ends_with(':') {
        let label_name = line.trim_end_matches(':').to_string();
        out.push(Instruction::Label(label_name));
        return Ok(out);
    }

    // debug_value instructions → skip
    if line.starts_with("debug_value") {
        return Ok(out);
    }

    // For unrecognized SIL, emit a load of 0 as a safe default
    if line.starts_with("%") || line.contains("=") {
        out.push(Instruction::LoadInt(0));
        // Try to capture result register
        if let Some(before_eq) = line.split('=').next() {
            let reg = before_eq.trim();
            if reg.starts_with('%') {
                let slot = *local_counter;
                *local_counter += 1;
                value_map.insert(reg.to_string(), slot);
                out.push(Instruction::Store(slot));
            }
        }
    }

    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn lower_simple_integer_literal() {
        let mut local_counter = 0;
        let mut value_map = HashMap::new();
        let mut label_counter = 0;

        let insts = parse_sil_instruction_to_bytecode(
            "integer_literal $Builtin.Int64, 42",
            &mut local_counter,
            &mut value_map,
            &mut label_counter,
        )
        .unwrap();

        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0], Instruction::LoadInt(42));
    }

    #[test]
    fn lower_return() {
        let mut local_counter = 0;
        let mut value_map = HashMap::new();
        let mut label_counter = 0;

        let insts = parse_sil_instruction_to_bytecode(
            "return",
            &mut local_counter,
            &mut value_map,
            &mut label_counter,
        )
        .unwrap();

        assert_eq!(insts.len(), 1);
        assert_eq!(insts[0], Instruction::Return);
    }

    #[test]
    fn lower_sil_artifact_to_bytecode() {
        let artifact = SilArtifact {
            function_id: "main".to_string(),
            cfg_blocks: vec!["entry".to_string()],
            instructions: vec![
                "integer_literal $Builtin.Int64, 42".to_string(),
                "return".to_string(),
            ],
            instruction_callers: vec![],
        };

        let module = lower_sil_to_bytecode(&artifact).unwrap();
        assert_eq!(module.entry_point, "main");
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name, "main");
    }
}
