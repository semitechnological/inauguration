//! Lower SIL to bytecode for execution or JIT compilation.

use crate::bytecode::{BytecodeFunction, BytecodeModule, Instruction};
use crate::hybrid_sil::SilArtifact;
use std::collections::HashMap;

/// Lower a SIL artifact to bytecode.
/// Handles multiple functions by grouping instructions under sil @ headers.
pub fn lower_sil_to_bytecode(artifact: &SilArtifact) -> Result<BytecodeModule, String> {
    let mut module = BytecodeModule::new(artifact.function_id.clone());

    // Group instructions by function
    let mut functions_map: HashMap<String, Vec<String>> = HashMap::new();
    let mut current_func = artifact.function_id.clone();

    for (idx, line) in artifact.instructions.iter().enumerate() {
        let line = line.trim();

        // Check if instruction_callers indicates a function boundary
        if idx < artifact.instruction_callers.len()
            && artifact.instruction_callers[idx] != current_func
        {
            current_func = artifact.instruction_callers[idx].clone();
        }

        functions_map
            .entry(current_func.clone())
            .or_insert_with(Vec::new)
            .push(line.to_string());
    }

    // Lower each function
    for (func_name, instructions) in functions_map {
        let bytecode_func = lower_function(&func_name, &instructions)?;
        module.add_function(bytecode_func);
    }

    Ok(module)
}

/// Lower a single function to bytecode.
fn lower_function(name: &str, instructions: &[String]) -> Result<BytecodeFunction, String> {
    let mut bytecode = Vec::new();
    let mut local_counter = 0;
    let mut value_map: HashMap<String, usize> = HashMap::new();
    let mut function_refs: HashMap<String, String> = HashMap::new();
    let mut _label_counter = 0;

    for line in instructions {
        let line = line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }

        // Parse SIL instructions and convert to bytecode
        if let Ok(insts) = parse_sil_instruction_to_bytecode(
            line,
            &mut local_counter,
            &mut value_map,
            &mut function_refs,
            &mut _label_counter,
        ) {
            bytecode.extend(insts);
        }
    }

    // Always end with return
    if !matches!(bytecode.last(), Some(Instruction::Return)) {
        bytecode.push(Instruction::Return);
    }

    Ok(BytecodeFunction {
        name: name.to_string(),
        instructions: bytecode,
        local_count: local_counter,
    })
}

/// Parse a single SIL instruction and emit bytecode equivalent(s).
fn parse_sil_instruction_to_bytecode(
    line: &str,
    local_counter: &mut usize,
    value_map: &mut HashMap<String, usize>,
    function_refs: &mut HashMap<String, String>,
    _label_counter: &mut usize,
) -> Result<Vec<Instruction>, String> {
    let mut out = Vec::new();
    let line = line.trim();

    // Skip empty and comment lines
    if line.is_empty() || line.starts_with("//") {
        return Ok(out);
    }

    // %0 = integer_literal $Builtin.Int64, 42
    if line.contains("=") && line.contains("integer_literal") {
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
    }

    // integer_literal $Builtin.Int64, 42 (standalone)
    if let Some(rest) = line.strip_prefix("integer_literal $Builtin.Int64,") {
        if let Ok(n) = rest.trim().parse::<i64>() {
            out.push(Instruction::LoadInt(n));
            return Ok(out);
        }
    }

    if line.contains("=") && line.contains("argument ") {
        if let Some(before_eq) = line.split('=').next() {
            let reg = before_eq.trim();
            if reg.starts_with('%') {
                let slot = *local_counter;
                *local_counter += 1;
                value_map.insert(reg.to_string(), slot);
                return Ok(out);
            }
        }
    }

    // %0 = apply %1(%2, %3) : $... (function call)
    if line.contains("= apply") {
        if let Some(eq_split) = line.split('=').nth(1) {
            if let Some(apply_rest) = eq_split.trim().strip_prefix("apply").map(str::trim) {
                // Extract function ref
                if let Some(paren_idx) = apply_rest.find('(') {
                    let func_ref = &apply_rest[..paren_idx].trim();

                    // Extract arguments between parens
                    if let Some(close_paren) = apply_rest.find(')') {
                        let args_str = &apply_rest[paren_idx + 1..close_paren];
                        for arg in args_str.split(',') {
                            let arg = arg.trim();
                            if arg.starts_with('%') {
                                if let Some(slot) = value_map.get(arg) {
                                    out.push(Instruction::Load(*slot));
                                }
                            }
                        }
                        let callee = function_refs
                            .get(*func_ref)
                            .cloned()
                            .unwrap_or_else(|| "user_func".to_string());
                        let argc = args_str
                            .split(',')
                            .map(str::trim)
                            .filter(|arg| !arg.is_empty())
                            .count();
                        out.push(Instruction::CallFunction(callee, argc));

                        // Store result
                        if let Some(before_eq) = line.split('=').next() {
                            let res_reg = before_eq.trim();
                            if res_reg.starts_with('%') {
                                let slot = *local_counter;
                                *local_counter += 1;
                                value_map.insert(res_reg.to_string(), slot);
                                out.push(Instruction::Store(slot));
                            }
                        }
                    }
                    return Ok(out);
                }
            }
        }
    }

    // function_ref @helper : $...
    if line.contains("function_ref @") {
        if let Some(rest) = line.split("function_ref @").nth(1) {
            let func_name = rest
                .split(|c: char| c.is_whitespace() || c == ':' || c == '(')
                .next()
                .unwrap_or("?");
            if let Some(before_eq) = line.split('=').next() {
                let reg = before_eq.trim();
                if reg.starts_with('%') {
                    function_refs.insert(reg.to_string(), func_name.to_string());
                    let slot = *local_counter;
                    *local_counter += 1;
                    value_map.insert(reg.to_string(), slot);
                    out.push(Instruction::LoadString(func_name.to_string()));
                    out.push(Instruction::Store(slot));
                    return Ok(out);
                }
            }
            out.push(Instruction::LoadString(func_name.to_string()));
        }
        return Ok(out);
    }

    // return %0 or return
    if line.starts_with("return") {
        if let Some(rest) = line.strip_prefix("return").map(str::trim) {
            if !rest.is_empty() && rest.starts_with('%') {
                let reg = rest
                    .split(|c: char| c.is_whitespace() || c == ':')
                    .next()
                    .unwrap_or(rest);
                // Load the return value
                if let Some(slot) = value_map.get(reg) {
                    out.push(Instruction::Load(*slot));
                }
            }
        }
        out.push(Instruction::Return);
        return Ok(out);
    }

    // cond_br %0, bb1, bb2 (conditional branch)
    if line.starts_with("cond_br") {
        if let Some(rest) = line.strip_prefix("cond_br").map(str::trim) {
            let parts: Vec<&str> = rest.split(',').collect();
            if parts.len() >= 3 {
                let cond_reg = parts[0].trim();
                let true_label = parts[1].trim();
                let false_label = parts[2].trim();

                // Load condition
                if let Some(slot) = value_map.get(cond_reg) {
                    out.push(Instruction::Load(*slot));
                }

                // Jump if true
                out.push(Instruction::JumpIfTrue(true_label.to_string()));
                // Jump to false label unconditionally
                out.push(Instruction::Jump(false_label.to_string()));
            }
        }
        return Ok(out);
    }

    // br bb1 (unconditional branch)
    if line.starts_with("br ") {
        if let Some(label) = line.strip_prefix("br ").map(str::trim) {
            out.push(Instruction::Jump(label.to_string()));
        }
        return Ok(out);
    }

    // bb0: / bb1: → Label
    if line.ends_with(':') {
        let label_name = line.trim_end_matches(':').to_string();
        out.push(Instruction::Label(label_name));
        return Ok(out);
    }

    // Skip debug_value
    if line.starts_with("debug_value") {
        return Ok(out);
    }

    // For other register assignments, emit a safe default (load 0)
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
        let mut function_refs = HashMap::new();
        let mut label_counter = 0;

        let insts = parse_sil_instruction_to_bytecode(
            "integer_literal $Builtin.Int64, 42",
            &mut local_counter,
            &mut value_map,
            &mut function_refs,
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
        let mut function_refs = HashMap::new();
        let mut label_counter = 0;

        let insts = parse_sil_instruction_to_bytecode(
            "return",
            &mut local_counter,
            &mut value_map,
            &mut function_refs,
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
            functions: vec![],
        };

        let module = lower_sil_to_bytecode(&artifact).unwrap();
        assert_eq!(module.entry_point, "main");
        assert_eq!(module.functions.len(), 1);
        assert_eq!(module.functions[0].name, "main");
    }

    #[test]
    fn lowers_apply_to_referenced_function_name() {
        let artifact = SilArtifact {
            function_id: "main".to_string(),
            cfg_blocks: vec!["entry".to_string()],
            instructions: vec![
                "%0 = function_ref @helper : $@convention(thin)".to_string(),
                "%1 = apply %0() : $@convention(thin)".to_string(),
                "return".to_string(),
            ],
            instruction_callers: vec![],
            functions: vec![],
        };

        let module = lower_sil_to_bytecode(&artifact).unwrap();
        let main = module
            .functions
            .iter()
            .find(|function| function.name == "main")
            .unwrap();
        assert!(
            main.instructions
                .iter()
                .any(|inst| matches!(inst, Instruction::CallFunction(name, 0) if name == "helper"))
        );
    }

    #[test]
    fn lowers_argument_register_to_local_slot() {
        let artifact = SilArtifact {
            function_id: "helper".to_string(),
            cfg_blocks: vec!["entry".to_string()],
            instructions: vec![
                "%0 = argument 0 : $Builtin.Int64".to_string(),
                "return %0 : $Builtin.Int64".to_string(),
            ],
            instruction_callers: vec![],
            functions: vec![],
        };

        let module = lower_sil_to_bytecode(&artifact).unwrap();
        let helper = module
            .functions
            .iter()
            .find(|function| function.name == "helper")
            .unwrap();
        assert_eq!(helper.local_count, 1);
        assert!(matches!(
            helper.instructions.first(),
            Some(Instruction::Load(0))
        ));
    }
}
