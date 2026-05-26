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
    let mut value_ints: HashMap<String, i64> = HashMap::new();
    let mut value_bools: HashMap<String, bool> = HashMap::new();
    let mut var_slots: HashMap<String, usize> = HashMap::new();
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
            &mut value_ints,
            &mut value_bools,
            &mut var_slots,
            &mut function_refs,
            &mut _label_counter,
        ) {
            bytecode.extend(insts);
        }
    }

    optimize_bytecode(&mut bytecode);

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

fn assignment(line: &str) -> Option<(&str, &str)> {
    let (lhs, rhs) = line.split_once('=')?;
    let reg = lhs.trim();
    if reg.starts_with('%') {
        Some((reg, rhs.trim()))
    } else {
        None
    }
}

fn store_register(
    reg: &str,
    local_counter: &mut usize,
    value_map: &mut HashMap<String, usize>,
    out: &mut Vec<Instruction>,
) {
    let slot = *local_counter;
    *local_counter += 1;
    value_map.insert(reg.to_string(), slot);
    out.push(Instruction::Store(slot));
}

fn variable_slot(
    name: &str,
    local_counter: &mut usize,
    var_slots: &mut HashMap<String, usize>,
) -> usize {
    if let Some(slot) = var_slots.get(name) {
        *slot
    } else {
        let slot = *local_counter;
        *local_counter += 1;
        var_slots.insert(name.to_string(), slot);
        slot
    }
}

fn parse_string_payload(raw: &str) -> String {
    raw.trim()
        .trim_matches('"')
        .replace("\\\"", "\"")
        .replace("\\\\", "\\")
}

fn parse_quoted_op_and_args(rest: &str) -> (String, Vec<&str>) {
    let rest = rest.trim();
    if let Some(after_open) = rest.strip_prefix('"')
        && let Some(end) = after_open.find('"')
    {
        let op = after_open[..end].to_string();
        let args = after_open[end + 1..]
            .split(',')
            .flat_map(str::split_whitespace)
            .filter(|part| part.starts_with('%'))
            .collect();
        return (op, args);
    }
    let mut parts = rest.split_whitespace();
    let op = parts.next().unwrap_or("").to_string();
    let args = parts
        .flat_map(|part| part.split(','))
        .map(str::trim)
        .filter(|part| part.starts_with('%'))
        .collect();
    (op, args)
}

fn parse_struct_init_payload(rest: &str) -> (String, Vec<(String, String)>) {
    let mut parts = rest.splitn(2, char::is_whitespace);
    let name = parts.next().unwrap_or("").to_string();
    let fields = parts
        .next()
        .unwrap_or("")
        .split(',')
        .filter_map(|part| {
            let (field, source) = part.trim().split_once(':')?;
            Some((field.trim().to_string(), source.trim().to_string()))
        })
        .collect();
    (name, fields)
}

fn is_builtin_function(name: &str) -> bool {
    matches!(
        name,
        "print" | "print_int" | "print_string" | "to_int" | "to_string" | "len"
    )
}

fn fold_int_binop(op: &str, lhs: i64, rhs: i64) -> Option<i64> {
    match op {
        "+" => lhs.checked_add(rhs),
        "-" => lhs.checked_sub(rhs),
        "*" => lhs.checked_mul(rhs),
        "/" if rhs != 0 => lhs.checked_div(rhs),
        "%" if rhs != 0 => lhs.checked_rem(rhs),
        _ => None,
    }
}

fn fold_bool_binop(
    op: &str,
    lhs_i: Option<i64>,
    rhs_i: Option<i64>,
    lhs_b: Option<bool>,
    rhs_b: Option<bool>,
) -> Option<bool> {
    match op {
        "&&" => Some(lhs_b? && rhs_b?),
        "||" => Some(lhs_b? || rhs_b?),
        "==" => {
            if let (Some(lhs), Some(rhs)) = (lhs_b, rhs_b) {
                return Some(lhs == rhs);
            }
            Some(lhs_i? == rhs_i?)
        }
        "!=" => {
            if let (Some(lhs), Some(rhs)) = (lhs_b, rhs_b) {
                return Some(lhs != rhs);
            }
            Some(lhs_i? != rhs_i?)
        }
        "<" => Some(lhs_i? < rhs_i?),
        ">" => Some(lhs_i? > rhs_i?),
        "<=" => Some(lhs_i? <= rhs_i?),
        ">=" => Some(lhs_i? >= rhs_i?),
        _ => None,
    }
}

fn clear_value_constants(
    reg: &str,
    value_ints: &mut HashMap<String, i64>,
    value_bools: &mut HashMap<String, bool>,
) {
    value_ints.remove(reg);
    value_bools.remove(reg);
}

fn optimize_bytecode(bytecode: &mut Vec<Instruction>) {
    let mut optimized = Vec::with_capacity(bytecode.len());
    let mut idx = 0;
    while idx < bytecode.len() {
        if let Some(Instruction::Jump(target)) = bytecode.get(idx)
            && let Some(Instruction::Label(label)) = bytecode.get(idx + 1)
            && target == label
        {
            idx += 1;
            continue;
        }
        if let Some(Instruction::Store(slot)) = bytecode.get(idx)
            && let Some(Instruction::Load(load_slot)) = bytecode.get(idx + 1)
            && slot == load_slot
            && !bytecode[idx + 2..]
                .iter()
                .any(|inst| matches!(inst, Instruction::Load(later_slot) if later_slot == slot))
        {
            idx += 2;
            continue;
        }
        optimized.push(bytecode[idx].clone());
        idx += 1;
    }
    *bytecode = optimized;
}

/// Parse a single SIL instruction and emit bytecode equivalent(s).
fn parse_sil_instruction_to_bytecode(
    line: &str,
    local_counter: &mut usize,
    value_map: &mut HashMap<String, usize>,
    value_ints: &mut HashMap<String, i64>,
    value_bools: &mut HashMap<String, bool>,
    var_slots: &mut HashMap<String, usize>,
    function_refs: &mut HashMap<String, String>,
    _label_counter: &mut usize,
) -> Result<Vec<Instruction>, String> {
    let mut out = Vec::new();
    let line = line.trim();

    // Skip empty and comment lines
    if line.is_empty() || line.starts_with("//") {
        return Ok(out);
    }

    if let Some(label) = line.strip_prefix("label ").map(str::trim) {
        out.push(Instruction::Label(label.to_string()));
        return Ok(out);
    }

    if let Some((reg, rhs)) = assignment(line) {
        if let Some(name) = rhs.strip_prefix("load_var ").map(str::trim) {
            if let Some(slot) = var_slots.get(name) {
                out.push(Instruction::Load(*slot));
            } else {
                out.push(Instruction::LoadNil);
            }
            store_register(reg, local_counter, value_map, &mut out);
            clear_value_constants(reg, value_ints, value_bools);
            return Ok(out);
        }
        if let Some(s) = rhs.strip_prefix("string_literal ").map(str::trim) {
            out.push(Instruction::LoadString(parse_string_payload(s)));
            store_register(reg, local_counter, value_map, &mut out);
            clear_value_constants(reg, value_ints, value_bools);
            return Ok(out);
        }
        if let Some(b) = rhs.strip_prefix("bool_literal ").map(str::trim) {
            let b = b == "true";
            out.push(Instruction::LoadBool(b));
            store_register(reg, local_counter, value_map, &mut out);
            value_ints.remove(reg);
            value_bools.insert(reg.to_string(), b);
            return Ok(out);
        }
        if let Some(rest) = rhs.strip_prefix("builtin_binop ").map(str::trim) {
            let (op, args) = parse_quoted_op_and_args(rest);
            if args.len() == 2
                && let (Some(lhs), Some(rhs)) = (value_ints.get(args[0]), value_ints.get(args[1]))
                && let Some(n) = fold_int_binop(&op, *lhs, *rhs)
            {
                out.push(Instruction::LoadInt(n));
                store_register(reg, local_counter, value_map, &mut out);
                value_ints.insert(reg.to_string(), n);
                value_bools.remove(reg);
                return Ok(out);
            }
            if args.len() == 2 {
                let lhs = args[0];
                let rhs = args[1];
                if let Some(b) = fold_bool_binop(
                    &op,
                    value_ints.get(lhs).copied(),
                    value_ints.get(rhs).copied(),
                    value_bools.get(lhs).copied(),
                    value_bools.get(rhs).copied(),
                ) {
                    out.push(Instruction::LoadBool(b));
                    store_register(reg, local_counter, value_map, &mut out);
                    value_ints.remove(reg);
                    value_bools.insert(reg.to_string(), b);
                    return Ok(out);
                }
            }
            for arg in args {
                if let Some(slot) = value_map.get(arg) {
                    out.push(Instruction::Load(*slot));
                }
            }
            out.push(Instruction::BinOp(op));
            store_register(reg, local_counter, value_map, &mut out);
            clear_value_constants(reg, value_ints, value_bools);
            return Ok(out);
        }
        if let Some(rest) = rhs.strip_prefix("builtin_unop ").map(str::trim) {
            let (op, args) = parse_quoted_op_and_args(rest);
            if let Some(arg) = args.first() {
                if op == "-"
                    && let Some(n) = value_ints.get(*arg).and_then(|n| n.checked_neg())
                {
                    out.push(Instruction::LoadInt(n));
                    store_register(reg, local_counter, value_map, &mut out);
                    value_ints.insert(reg.to_string(), n);
                    value_bools.remove(reg);
                    return Ok(out);
                }
                if op == "!"
                    && let Some(b) = value_bools.get(*arg)
                {
                    out.push(Instruction::LoadBool(!b));
                    store_register(reg, local_counter, value_map, &mut out);
                    value_ints.remove(reg);
                    value_bools.insert(reg.to_string(), !b);
                    return Ok(out);
                }
                if let Some(slot) = value_map.get(*arg) {
                    out.push(Instruction::Load(*slot));
                }
            }
            out.push(Instruction::UnOp(op));
            store_register(reg, local_counter, value_map, &mut out);
            clear_value_constants(reg, value_ints, value_bools);
            return Ok(out);
        }
        if let Some(rest) = rhs.strip_prefix("struct_init ").map(str::trim) {
            let (name, fields) = parse_struct_init_payload(rest);
            for (_, source_reg) in &fields {
                if let Some(slot) = value_map.get(source_reg.as_str()) {
                    out.push(Instruction::Load(*slot));
                }
            }
            out.push(Instruction::StructInit(
                name,
                fields.into_iter().map(|(field, _)| field).collect(),
            ));
            store_register(reg, local_counter, value_map, &mut out);
            clear_value_constants(reg, value_ints, value_bools);
            return Ok(out);
        }
        if let Some(rest) = rhs.strip_prefix("field_access ").map(str::trim) {
            let mut parts = rest.split_whitespace();
            let source_reg = parts.next().ok_or("parse error")?;
            let field_name = parts.next().ok_or("parse error")?;
            if let Some(slot) = value_map.get(source_reg) {
                out.push(Instruction::Load(*slot));
            }
            out.push(Instruction::FieldAccess(field_name.to_string()));
            store_register(reg, local_counter, value_map, &mut out);
            clear_value_constants(reg, value_ints, value_bools);
            return Ok(out);
        }
        if let Some(rest) = rhs.strip_prefix("array_init").map(str::trim) {
            let args = rest
                .split(',')
                .map(str::trim)
                .filter(|arg| !arg.is_empty())
                .collect::<Vec<_>>();
            for arg in &args {
                if let Some(slot) = value_map.get(*arg) {
                    out.push(Instruction::Load(*slot));
                }
            }
            out.push(Instruction::ArrayInit(args.len()));
            store_register(reg, local_counter, value_map, &mut out);
            clear_value_constants(reg, value_ints, value_bools);
            return Ok(out);
        }
        if let Some(rest) = rhs.strip_prefix("index_access ").map(str::trim) {
            for arg in rest.split(',').map(str::trim).filter(|arg| !arg.is_empty()) {
                if let Some(slot) = value_map.get(arg) {
                    out.push(Instruction::Load(*slot));
                }
            }
            out.push(Instruction::IndexAccess);
            store_register(reg, local_counter, value_map, &mut out);
            clear_value_constants(reg, value_ints, value_bools);
            return Ok(out);
        }
    }

    if let Some(rest) = line.strip_prefix("store_var ").map(str::trim) {
        let mut parts = rest.split_whitespace();
        let name = parts.next().ok_or("parse error")?;
        let reg = parts.next().ok_or("parse error")?;
        let source = value_map.get(reg).ok_or("parse error")?;
        out.push(Instruction::Load(*source));
        let slot = variable_slot(name, local_counter, var_slots);
        out.push(Instruction::Store(slot));
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
                            value_ints.insert(reg.to_string(), n);
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
                clear_value_constants(reg, value_ints, value_bools);
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
                        if is_builtin_function(&callee) {
                            out.push(Instruction::CallBuiltin(callee, argc));
                        } else {
                            out.push(Instruction::CallFunction(callee, argc));
                        }

                        // Store result
                        if let Some(before_eq) = line.split('=').next() {
                            let res_reg = before_eq.trim();
                            if res_reg.starts_with('%') {
                                store_register(res_reg, local_counter, value_map, &mut out);
                                clear_value_constants(res_reg, value_ints, value_bools);
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
                    clear_value_constants(reg, value_ints, value_bools);
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
                clear_value_constants(reg, value_ints, value_bools);
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
        let mut value_ints = HashMap::new();
        let mut value_bools = HashMap::new();
        let mut var_slots = HashMap::new();
        let mut function_refs = HashMap::new();
        let mut label_counter = 0;

        let insts = parse_sil_instruction_to_bytecode(
            "integer_literal $Builtin.Int64, 42",
            &mut local_counter,
            &mut value_map,
            &mut value_ints,
            &mut value_bools,
            &mut var_slots,
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
        let mut value_ints = HashMap::new();
        let mut value_bools = HashMap::new();
        let mut var_slots = HashMap::new();
        let mut function_refs = HashMap::new();
        let mut label_counter = 0;

        let insts = parse_sil_instruction_to_bytecode(
            "return",
            &mut local_counter,
            &mut value_map,
            &mut value_ints,
            &mut value_bools,
            &mut var_slots,
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

    #[test]
    fn removes_adjacent_store_load_pair() {
        let artifact = SilArtifact {
            function_id: "main".to_string(),
            cfg_blocks: vec!["entry".to_string()],
            instructions: vec![
                "%0 = integer_literal $Builtin.Int64, 4".to_string(),
                "store_var x %0".to_string(),
                "%1 = load_var x".to_string(),
                "return %1 : $Builtin.Int64".to_string(),
            ],
            instruction_callers: vec![],
            functions: vec![],
        };

        let module = lower_sil_to_bytecode(&artifact).unwrap();
        let main = module.find_function("main").unwrap();

        assert!(
            !main.instructions.windows(2).any(
                |pair| matches!(pair, [Instruction::Store(a), Instruction::Load(b)] if a == b)
            )
        );
    }

    #[test]
    fn folds_literal_binop_bytecode() {
        let artifact = SilArtifact {
            function_id: "main".to_string(),
            cfg_blocks: vec!["entry".to_string()],
            instructions: vec![
                "%0 = integer_literal $Builtin.Int64, 2".to_string(),
                "%1 = integer_literal $Builtin.Int64, 3".to_string(),
                "%2 = builtin_binop \"+\" %0, %1".to_string(),
                "return %2 : $Builtin.Int64".to_string(),
            ],
            instruction_callers: vec![],
            functions: vec![],
        };

        let module = lower_sil_to_bytecode(&artifact).unwrap();
        let main = module.find_function("main").unwrap();

        assert!(main.instructions.contains(&Instruction::LoadInt(5)));
        assert!(
            !main
                .instructions
                .iter()
                .any(|inst| matches!(inst, Instruction::BinOp(op) if op == "+"))
        );
    }

    #[test]
    fn folds_literal_unary_and_bool_bytecode() {
        let artifact = SilArtifact {
            function_id: "main".to_string(),
            cfg_blocks: vec!["entry".to_string()],
            instructions: vec![
                "%0 = bool_literal false".to_string(),
                "%1 = builtin_unop \"!\" %0".to_string(),
                "%2 = integer_literal $Builtin.Int64, 2".to_string(),
                "%3 = integer_literal $Builtin.Int64, 2".to_string(),
                "%4 = builtin_binop \"==\" %2, %3".to_string(),
                "%5 = builtin_binop \"&&\" %1, %4".to_string(),
                "return %5 : $Builtin.Int1".to_string(),
            ],
            instruction_callers: vec![],
            functions: vec![],
        };

        let module = lower_sil_to_bytecode(&artifact).unwrap();
        let main = module.find_function("main").unwrap();

        assert!(main.instructions.contains(&Instruction::LoadBool(true)));
        assert!(
            !main
                .instructions
                .iter()
                .any(|inst| matches!(inst, Instruction::UnOp(op) if op == "!"))
        );
        assert!(
            !main
                .instructions
                .iter()
                .any(|inst| matches!(inst, Instruction::BinOp(op) if op == "&&" || op == "=="))
        );
    }

    #[test]
    fn removes_jump_to_immediately_following_label() {
        let artifact = SilArtifact {
            function_id: "main".to_string(),
            cfg_blocks: vec!["entry".to_string()],
            instructions: vec![
                "br bb1".to_string(),
                "label bb1".to_string(),
                "return".to_string(),
            ],
            instruction_callers: vec![],
            functions: vec![],
        };

        let module = lower_sil_to_bytecode(&artifact).unwrap();
        let main = module.find_function("main").unwrap();

        assert!(!matches!(
            main.instructions.as_slice(),
            [Instruction::Jump(target), Instruction::Label(label), ..] if target == label
        ));
    }
}
