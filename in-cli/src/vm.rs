//! Bytecode VM: executes compiled bytecode programs.

use crate::bytecode::{BytecodeFunction, BytecodeModule, Instruction, Value};
use std::collections::HashMap;

/// Execution context for a bytecode function.
pub struct CallFrame {
    pub locals: Vec<Value>,
    pub ip: usize, // instruction pointer
}

/// The bytecode runtime (stack-based VM).
pub struct BytecodeVM {
    pub stack: Vec<Value>,
    pub frames: Vec<CallFrame>,
    pub module: BytecodeModule,
    pub globals: HashMap<String, Value>,
    pub error_state: Option<Value>,
}

impl BytecodeVM {
    pub fn new(module: BytecodeModule) -> Self {
        BytecodeVM {
            stack: Vec::new(),
            frames: Vec::new(),
            module,
            globals: HashMap::new(),
            error_state: None,
        }
    }

    /// Run the bytecode program.
    pub fn run(&mut self) -> Result<Value, String> {
        let entry = self.module.entry_point.clone();
        let result = self.call_function(&entry, vec![]);
        if let Some(ref err) = self.error_state {
            return Err(format!("uncaught exception: {}", err.to_string_display()));
        }
        result
    }

    /// Call a user-defined function.
    fn call_function(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        let func = self
            .module
            .find_function(name)
            .ok_or(format!("function not found: {}", name))?
            .clone();

        let mut frame = CallFrame {
            locals: vec![Value::Nil; func.local_count],
            ip: 0,
        };

        // Load arguments into locals
        for (i, arg) in args.iter().enumerate() {
            if i < frame.locals.len() {
                frame.locals[i] = arg.clone();
            }
        }

        self.frames.push(frame);
        self.execute_function(&func)?;
        self.frames.pop();

        if self.error_state.is_some() {
            return Ok(Value::Nil);
        }

        Ok(if self.stack.is_empty() {
            Value::Nil
        } else {
            self.stack.pop().unwrap()
        })
    }

    /// Execute instructions for a function.
    fn execute_function(&mut self, func: &BytecodeFunction) -> Result<(), String> {
        let frame_idx = self.frames.len() - 1;
        let label_map = build_label_map(&func.instructions);

        loop {
            let frame = &self.frames[frame_idx];
            let ip = frame.ip;

            if ip >= func.instructions.len() || self.error_state.is_some() {
                break;
            }

            let inst = func.instructions[ip].clone();
            self.frames[frame_idx].ip += 1;

            match inst {
                Instruction::LoadInt(n) => {
                    self.stack.push(Value::Int(n));
                }
                Instruction::LoadString(s) => {
                    self.stack.push(Value::String(s));
                }
                Instruction::LoadBool(b) => {
                    self.stack.push(Value::Bool(b));
                }
                Instruction::LoadNil => {
                    self.stack.push(Value::Nil);
                }
                Instruction::CallBuiltin(builtin_name, argc) => {
                    let args = self.pop_n(argc)?;
                    let result = self.call_builtin(&builtin_name, args)?;
                    self.stack.push(result);
                }
                Instruction::CallFunction(fn_name, argc) => {
                    // Pop arguments from stack
                    let args = self.pop_n(argc)?;
                    // Call the function properly (manages frame stack)
                    let result = self.call_function(&fn_name, args)?;
                    // Push result back on stack
                    self.stack.push(result);
                }
                Instruction::Return => {
                    break;
                }
                Instruction::BinOp(op) => {
                    let rhs = self.stack.pop().ok_or("stack underflow")?;
                    let lhs = self.stack.pop().ok_or("stack underflow")?;
                    let result = self.apply_binop(&op, lhs, rhs)?;
                    self.stack.push(result);
                }
                Instruction::UnOp(op) => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    let result = self.apply_unop(&op, val)?;
                    self.stack.push(result);
                }
                Instruction::StructInit(name, fields) => {
                    let values = self.pop_n(fields.len())?;
                    self.stack.push(Value::Struct {
                        name,
                        fields: fields.into_iter().zip(values).collect(),
                    });
                }
                Instruction::FieldAccess(name) => {
                    let value = self.stack.pop().ok_or("stack underflow")?;
                    if let Value::Struct { fields, .. } = value {
                        let field = fields
                            .into_iter()
                            .find(|(field, _)| field == &name)
                            .map(|(_, value)| value)
                            .ok_or(format!("field not found: {}", name))?;
                        self.stack.push(field);
                    } else {
                        return Err(format!("field access on non-struct: {}", name));
                    }
                }
                Instruction::ArrayInit(len) => {
                    let values = self.pop_n(len)?;
                    self.stack.push(Value::Array(values));
                }
                Instruction::IndexAccess => {
                    let index = self.stack.pop().ok_or("stack underflow")?.to_int();
                    let value = self.stack.pop().ok_or("stack underflow")?;
                    if index < 0 {
                        return Err(format!("array index out of bounds: {}", index));
                    }
                    if let Value::Array(values) = value {
                        let item = values
                            .get(index as usize)
                            .cloned()
                            .ok_or(format!("array index out of bounds: {}", index))?;
                        self.stack.push(item);
                    } else {
                        return Err("index access on non-array".to_string());
                    }
                }
                Instruction::IndexSet(slot) => {
                    let value = self.stack.pop().ok_or("stack underflow")?;
                    let index = self.stack.pop().ok_or("stack underflow")?.to_int();
                    if index < 0 {
                        return Err(format!("array index out of bounds: {}", index));
                    }
                    let Some(local) = self.frames[frame_idx].locals.get_mut(slot) else {
                        return Err(format!("invalid local slot: {}", slot));
                    };
                    if let Value::Array(values) = local {
                        let Some(item) = values.get_mut(index as usize) else {
                            return Err(format!("array index out of bounds: {}", index));
                        };
                        *item = value;
                    } else {
                        return Err("index assignment on non-array".to_string());
                    }
                }
                Instruction::Jump(label) => {
                    let target = label_map
                        .get(label.as_str())
                        .ok_or(format!("label not found: {}", label))?;
                    self.frames[frame_idx].ip = *target;
                }
                Instruction::JumpIfFalse(label) => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    if !val.to_bool() {
                        let target = label_map
                            .get(label.as_str())
                            .ok_or(format!("label not found: {}", label))?;
                        self.frames[frame_idx].ip = *target;
                    }
                }
                Instruction::JumpIfTrue(label) => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    if val.to_bool() {
                        let target = label_map
                            .get(label.as_str())
                            .ok_or(format!("label not found: {}", label))?;
                        self.frames[frame_idx].ip = *target;
                    }
                }
                Instruction::Label(_) => {
                    // Labels are no-ops at runtime
                }
                Instruction::Pop => {
                    self.stack.pop().ok_or("stack underflow")?;
                }
                Instruction::Dup => {
                    let val = self.stack.last().ok_or("stack underflow")?.clone();
                    self.stack.push(val);
                }
                Instruction::Store(slot) => {
                    let val = self.stack.pop().ok_or("stack underflow")?;
                    if slot < self.frames[frame_idx].locals.len() {
                        self.frames[frame_idx].locals[slot] = val;
                    } else {
                        return Err(format!("invalid local slot: {}", slot));
                    }
                }
                Instruction::Load(slot) => {
                    if slot < self.frames[frame_idx].locals.len() {
                        let val = self.frames[frame_idx].locals[slot].clone();
                        self.stack.push(val);
                    } else {
                        return Err(format!("invalid local slot: {}", slot));
                    }
                }
            }
        }

        Ok(())
    }

    /// Pop n values from the stack.
    fn pop_n(&mut self, n: usize) -> Result<Vec<Value>, String> {
        let mut vals = Vec::new();
        for _ in 0..n {
            vals.push(self.stack.pop().ok_or("stack underflow")?);
        }
        vals.reverse();
        Ok(vals)
    }

    /// Call a built-in function.
    fn call_builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Value, String> {
        match name {
            "print" => {
                for arg in args {
                    print!("{}", arg.to_string_display());
                }
                println!();
                Ok(Value::Nil)
            }
            "print_int" => {
                if let Some(arg) = args.first() {
                    print!("{}", arg.to_int());
                }
                Ok(Value::Nil)
            }
            "print_string" => {
                if let Some(arg) = args.first() {
                    print!("{}", arg.to_string_display());
                }
                Ok(Value::Nil)
            }
            "to_int" => {
                if let Some(arg) = args.first() {
                    Ok(Value::Int(arg.to_int()))
                } else {
                    Ok(Value::Int(0))
                }
            }
            "to_string" => {
                if let Some(arg) = args.first() {
                    Ok(Value::String(arg.to_string_display()))
                } else {
                    Ok(Value::String(String::new()))
                }
            }
            "len" => {
                if let Some(Value::String(s)) = args.first() {
                    Ok(Value::Int(s.len() as i64))
                } else {
                    Ok(Value::Int(0))
                }
            }
            "throw_error" => {
                let err_val = args
                    .into_iter()
                    .next()
                    .unwrap_or(Value::String("unhandled exception".to_string()));
                self.error_state = Some(err_val);
                Ok(Value::Nil)
            }
            _ => Err(format!("unknown builtin: {}", name)),
        }
    }

    /// Apply a binary operator.
    fn apply_binop(&self, op: &str, lhs: Value, rhs: Value) -> Result<Value, String> {
        match op {
            "==" => return Ok(Value::Bool(lhs == rhs)),
            "!=" => return Ok(Value::Bool(lhs != rhs)),
            "&&" => return Ok(Value::Bool(lhs.to_bool() && rhs.to_bool())),
            "||" => return Ok(Value::Bool(lhs.to_bool() || rhs.to_bool())),
            _ => {}
        }
        let l = lhs.to_int();
        let r = rhs.to_int();
        match op {
            "+" => Ok(Value::Int(l + r)),
            "-" => Ok(Value::Int(l - r)),
            "*" => Ok(Value::Int(l * r)),
            "/" => {
                if r == 0 {
                    Err("division by zero".to_string())
                } else {
                    Ok(Value::Int(l / r))
                }
            }
            "%" => {
                if r == 0 {
                    Err("division by zero".to_string())
                } else {
                    Ok(Value::Int(l % r))
                }
            }
            "<" => Ok(Value::Bool(l < r)),
            ">" => Ok(Value::Bool(l > r)),
            "<=" => Ok(Value::Bool(l <= r)),
            ">=" => Ok(Value::Bool(l >= r)),
            _ => Err(format!("unknown binop: {}", op)),
        }
    }

    /// Apply a unary operator.
    fn apply_unop(&self, op: &str, val: Value) -> Result<Value, String> {
        match op {
            "-" => Ok(Value::Int(-val.to_int())),
            "!" => Ok(Value::Bool(!val.to_bool())),
            _ => Err(format!("unknown unop: {}", op)),
        }
    }
}

/// Build a map from label names to instruction indices.
fn build_label_map(instructions: &[Instruction]) -> HashMap<&str, usize> {
    let mut map = HashMap::new();
    for (i, inst) in instructions.iter().enumerate() {
        if let Instruction::Label(name) = inst {
            map.insert(name.as_str(), i);
        }
    }
    map
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vm_simple_arithmetic() {
        let mut module = BytecodeModule::new("main".to_string());
        let func = BytecodeFunction {
            name: "main".to_string(),
            instructions: vec![
                Instruction::LoadInt(5),
                Instruction::LoadInt(3),
                Instruction::BinOp("+".to_string()),
                Instruction::Return,
            ],
            local_count: 0,
        };
        module.add_function(func);

        let mut vm = BytecodeVM::new(module);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(8));
    }

    #[test]
    fn vm_builtin_print() {
        let mut module = BytecodeModule::new("main".to_string());
        let func = BytecodeFunction {
            name: "main".to_string(),
            instructions: vec![
                Instruction::LoadInt(42),
                Instruction::CallBuiltin("print_int".to_string(), 1),
                Instruction::Return,
            ],
            local_count: 0,
        };
        module.add_function(func);

        let mut vm = BytecodeVM::new(module);
        let _ = vm.run(); // Should print "42"
    }

    #[test]
    fn vm_local_storage() {
        let mut module = BytecodeModule::new("main".to_string());
        let func = BytecodeFunction {
            name: "main".to_string(),
            instructions: vec![
                Instruction::LoadInt(10),
                Instruction::Store(0),
                Instruction::Load(0),
                Instruction::Return,
            ],
            local_count: 1,
        };
        module.add_function(func);

        let mut vm = BytecodeVM::new(module);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(10));
    }

    #[test]
    fn vm_struct_field_access() {
        let mut module = BytecodeModule::new("main".to_string());
        let func = BytecodeFunction {
            name: "main".to_string(),
            instructions: vec![
                Instruction::LoadInt(2),
                Instruction::LoadInt(5),
                Instruction::StructInit(
                    "Point".to_string(),
                    vec!["x".to_string(), "y".to_string()],
                ),
                Instruction::FieldAccess("y".to_string()),
                Instruction::Return,
            ],
            local_count: 0,
        };
        module.add_function(func);

        let mut vm = BytecodeVM::new(module);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(5));
    }
}
