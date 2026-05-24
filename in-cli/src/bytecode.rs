//! Simple stack-based bytecode IR and emitter.
//! 
//! Bytecode is a minimal intermediate representation that SIL can lower to,
//! enabling code generation without external compilers or complex backends.

use serde::{Deserialize, Serialize};

/// Runtime value on the stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    Int(i64),
    Bool(bool),
    String(String),
    Nil,
}

impl Value {
    pub fn to_int(&self) -> i64 {
        match self {
            Value::Int(n) => *n,
            Value::Bool(b) => if *b { 1 } else { 0 },
            Value::String(_) => 0,
            Value::Nil => 0,
        }
    }

    pub fn to_bool(&self) -> bool {
        match self {
            Value::Int(n) => *n != 0,
            Value::Bool(b) => *b,
            Value::String(s) => !s.is_empty(),
            Value::Nil => false,
        }
    }

    pub fn to_string_display(&self) -> String {
        match self {
            Value::Int(n) => n.to_string(),
            Value::Bool(b) => b.to_string(),
            Value::String(s) => s.clone(),
            Value::Nil => "nil".to_string(),
        }
    }
}

/// Bytecode instructions (stack-based).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Instruction {
    /// Load integer constant onto stack
    LoadInt(i64),
    /// Load string constant onto stack
    LoadString(String),
    /// Load boolean constant onto stack
    LoadBool(bool),
    /// Load nil
    LoadNil,
    /// Call built-in function (name, arg count)
    CallBuiltin(String, usize),
    /// Call user-defined function (name, arg count)
    CallFunction(String, usize),
    /// Return from function
    Return,
    /// Binary operation: pop 2 values, apply op, push result
    BinOp(String),
    /// Unary operation: pop 1 value, apply op, push result
    UnOp(String),
    /// Jump to label
    Jump(String),
    /// Jump if top of stack is false (pop value)
    JumpIfFalse(String),
    /// Jump if top of stack is true (pop value)
    JumpIfTrue(String),
    /// Label (no-op, marks position)
    Label(String),
    /// Pop from stack
    Pop,
    /// Duplicate top of stack
    Dup,
    /// Store in local (slot index)
    Store(usize),
    /// Load from local (slot index)
    Load(usize),
}

/// A function in bytecode form.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BytecodeFunction {
    pub name: String,
    pub instructions: Vec<Instruction>,
    pub local_count: usize,
}

/// A complete bytecode module.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BytecodeModule {
    pub functions: Vec<BytecodeFunction>,
    pub entry_point: String,
}

impl BytecodeModule {
    pub fn new(entry_point: String) -> Self {
        BytecodeModule {
            functions: Vec::new(),
            entry_point,
        }
    }

    pub fn add_function(&mut self, func: BytecodeFunction) {
        self.functions.push(func);
    }

    pub fn find_function(&self, name: &str) -> Option<&BytecodeFunction> {
        self.functions.iter().find(|f| f.name == name)
    }
}

/// Emit textual bytecode assembly (.bca format).
pub fn module_to_text(module: &BytecodeModule) -> String {
    let mut out = String::new();
    out.push_str(&format!("; Bytecode module (entry: {})\n", module.entry_point));
    out.push_str("; ---\n\n");

    for func in &module.functions {
        out.push_str(&format!("function @{}:\n", func.name));
        out.push_str(&format!("  locals: {}\n", func.local_count));
        for inst in &func.instructions {
            out.push_str(&format!("  {}\n", instruction_to_text(inst)));
        }
        out.push_str("\n");
    }

    out
}

fn instruction_to_text(inst: &Instruction) -> String {
    match inst {
        Instruction::LoadInt(n) => format!("load_int {}", n),
        Instruction::LoadString(s) => format!("load_string {:?}", s),
        Instruction::LoadBool(b) => format!("load_bool {}", b),
        Instruction::LoadNil => "load_nil".to_string(),
        Instruction::CallBuiltin(name, argc) => format!("call_builtin {} {}", name, argc),
        Instruction::CallFunction(name, argc) => format!("call {} {}", name, argc),
        Instruction::Return => "return".to_string(),
        Instruction::BinOp(op) => format!("binop {}", op),
        Instruction::UnOp(op) => format!("unop {}", op),
        Instruction::Jump(label) => format!("jmp {}", label),
        Instruction::JumpIfFalse(label) => format!("jmpf {}", label),
        Instruction::JumpIfTrue(label) => format!("jmpt {}", label),
        Instruction::Label(label) => format!("{}:", label),
        Instruction::Pop => "pop".to_string(),
        Instruction::Dup => "dup".to_string(),
        Instruction::Store(slot) => format!("store {}", slot),
        Instruction::Load(slot) => format!("load {}", slot),
    }
}

/// Parse textual bytecode assembly (.bca) back to module.
pub fn text_to_module(text: &str) -> Result<BytecodeModule, String> {
    let mut functions = Vec::new();
    let mut current_func: Option<BytecodeFunction> = None;
    let mut entry_point = "main".to_string();

    for line in text.lines() {
        let trimmed = line.trim();

        // Skip comments and empty lines
        if trimmed.is_empty() || trimmed.starts_with(';') {
            if trimmed.starts_with("; entry:") {
                entry_point = trimmed
                    .strip_prefix("; entry:")
                    .unwrap_or("main")
                    .trim()
                    .to_string();
            } else if let Some(rest) = trimmed.strip_prefix("; Bytecode module (entry:") {
                entry_point = rest.trim_end_matches(')').trim().to_string();
            }
            continue;
        }

        // Function header
        if trimmed.starts_with("function @") {
            if let Some(func) = current_func {
                functions.push(func);
            }
            let name = trimmed
                .strip_prefix("function @")
                .unwrap_or("")
                .trim_end_matches(':')
                .to_string();
            current_func = Some(BytecodeFunction {
                name,
                instructions: Vec::new(),
                local_count: 0,
            });
            continue;
        }

        // Parse locals declaration
        if trimmed.starts_with("locals:") {
            if let Some(ref mut func) = current_func {
                if let Ok(n) = trimmed
                    .strip_prefix("locals:")
                    .unwrap_or("0")
                    .trim()
                    .parse::<usize>()
                {
                    func.local_count = n;
                }
            }
            continue;
        }

        // Parse instruction
        if let Some(ref mut func) = current_func {
            if let Ok(inst) = parse_instruction(trimmed) {
                func.instructions.push(inst);
            }
        }
    }

    if let Some(func) = current_func {
        functions.push(func);
    }

    Ok(BytecodeModule {
        functions,
        entry_point,
    })
}

fn parse_instruction(line: &str) -> Result<Instruction, String> {
    let parts: Vec<&str> = line.split_whitespace().collect();
    if parts.is_empty() {
        return Err("empty instruction".to_string());
    }

    match parts[0] {
        "load_int" => {
            let n = parts
                .get(1)
                .and_then(|s| s.parse::<i64>().ok())
                .ok_or("parse error")?;
            Ok(Instruction::LoadInt(n))
        }
        "load_string" => {
            let s = parts
                .get(1)
                .map(|s| s.trim_matches('"').to_string())
                .ok_or("parse error")?;
            Ok(Instruction::LoadString(s))
        }
        "load_bool" => {
            let b = parts
                .get(1)
                .and_then(|s| s.parse::<bool>().ok())
                .ok_or("parse error")?;
            Ok(Instruction::LoadBool(b))
        }
        "load_nil" => Ok(Instruction::LoadNil),
        "call_builtin" => {
            let name = parts.get(1).ok_or("parse error")?.to_string();
            let argc = parts
                .get(2)
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or("parse error")?;
            Ok(Instruction::CallBuiltin(name, argc))
        }
        "call" => {
            let name = parts.get(1).ok_or("parse error")?.to_string();
            let argc = parts
                .get(2)
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or("parse error")?;
            Ok(Instruction::CallFunction(name, argc))
        }
        "return" => Ok(Instruction::Return),
        "binop" => {
            let op = parts.get(1).ok_or("parse error")?.to_string();
            Ok(Instruction::BinOp(op))
        }
        "unop" => {
            let op = parts.get(1).ok_or("parse error")?.to_string();
            Ok(Instruction::UnOp(op))
        }
        "jmp" => {
            let label = parts.get(1).ok_or("parse error")?.to_string();
            Ok(Instruction::Jump(label))
        }
        "jmpf" => {
            let label = parts.get(1).ok_or("parse error")?.to_string();
            Ok(Instruction::JumpIfFalse(label))
        }
        "jmpt" => {
            let label = parts.get(1).ok_or("parse error")?.to_string();
            Ok(Instruction::JumpIfTrue(label))
        }
        "pop" => Ok(Instruction::Pop),
        "dup" => Ok(Instruction::Dup),
        "store" => {
            let slot = parts
                .get(1)
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or("parse error")?;
            Ok(Instruction::Store(slot))
        }
        "load" => {
            let slot = parts
                .get(1)
                .and_then(|s| s.parse::<usize>().ok())
                .ok_or("parse error")?;
            Ok(Instruction::Load(slot))
        }
        s if s.ends_with(':') => {
            let label = s.trim_end_matches(':').to_string();
            Ok(Instruction::Label(label))
        }
        _ => Err(format!("unknown instruction: {}", parts[0])),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytecode_value_conversions() {
        assert_eq!(Value::Int(5).to_int(), 5);
        assert_eq!(Value::Bool(true).to_int(), 1);
        assert_eq!(Value::Bool(true).to_bool(), true);
        assert_eq!(Value::Int(0).to_bool(), false);
    }

    #[test]
    fn module_to_text_roundtrip() {
        let mut module = BytecodeModule::new("main".to_string());
        let func = BytecodeFunction {
            name: "main".to_string(),
            instructions: vec![
                Instruction::LoadInt(42),
                Instruction::CallBuiltin("print".to_string(), 1),
                Instruction::Return,
            ],
            local_count: 0,
        };
        module.add_function(func);

        let text = module_to_text(&module);
        assert!(text.contains("function @main"));
        assert!(text.contains("load_int 42"));
        assert!(text.contains("call_builtin print 1"));

        let parsed = text_to_module(&text).unwrap();
        assert_eq!(parsed.entry_point, "main");
    }

    #[test]
    fn parse_instruction_works() {
        let inst = parse_instruction("load_int 123").unwrap();
        assert_eq!(inst, Instruction::LoadInt(123));

        let inst = parse_instruction("call foo 2").unwrap();
        assert_eq!(inst, Instruction::CallFunction("foo".to_string(), 2));
    }
}
