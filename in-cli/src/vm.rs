//! Bytecode VM: executes compiled bytecode programs.

use crate::bytecode::{BytecodeFunction, BytecodeModule, CmpOp, Instruction, Value};
use crate::core_ir::FloatVal;
use std::collections::HashMap;

pub struct TryRegion {
    pub start_ip: usize,
    pub end_ip: usize,
    pub catch_label: String,
}

/// Execution context for a bytecode function.
pub struct CallFrame {
    pub locals: Vec<Value>,
    pub ip: usize,
    pub try_regions: Vec<TryRegion>,
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
        if let Some(runtime) = self.module.package_exports.get(name) {
            return crate::package_runtime::invoke_package_export(runtime, &args);
        }
        let func = self
            .module
            .find_function(name)
            .ok_or(format!("function not found: {}", name))?
            .clone();

        let mut frame = CallFrame {
            locals: vec![Value::Nil; func.local_count],
            ip: 0,
            try_regions: Vec::new(),
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

            if ip >= func.instructions.len() {
                break;
            }

            if self.error_state.is_some() {
                let regions = &self.frames[frame_idx].try_regions;
                if let Some(region) = regions
                    .iter()
                    .rev()
                    .find(|r| ip >= r.start_ip && ip < r.end_ip)
                {
                    let catch_label = region.catch_label.clone();
                    let catch_ip = label_map
                        .get(catch_label.as_str())
                        .copied()
                        .unwrap_or(usize::MAX);
                    self.frames[frame_idx].ip = catch_ip;
                    self.error_state = None;
                    continue;
                }
                break;
            }

            let inst = func.instructions[ip].clone();
            self.frames[frame_idx].ip += 1;

            match inst {
                Instruction::LoadInt(n) => {
                    self.stack.push(Value::Int(n));
                }
                Instruction::LoadFloat(f) => {
                    self.stack.push(Value::Float(f));
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
                    let results = self.call_builtin(&builtin_name, args)?;
                    for result in results {
                        self.stack.push(result);
                    }
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
                Instruction::FAdd => {
                    let rhs = self.stack.pop().ok_or("stack underflow")?.to_float();
                    let lhs = self.stack.pop().ok_or("stack underflow")?.to_float();
                    self.stack.push(Value::Float(FloatVal(lhs + rhs)));
                }
                Instruction::FSub => {
                    let rhs = self.stack.pop().ok_or("stack underflow")?.to_float();
                    let lhs = self.stack.pop().ok_or("stack underflow")?.to_float();
                    self.stack.push(Value::Float(FloatVal(lhs - rhs)));
                }
                Instruction::FMul => {
                    let rhs = self.stack.pop().ok_or("stack underflow")?.to_float();
                    let lhs = self.stack.pop().ok_or("stack underflow")?.to_float();
                    self.stack.push(Value::Float(FloatVal(lhs * rhs)));
                }
                Instruction::FDiv => {
                    let rhs = self.stack.pop().ok_or("stack underflow")?.to_float();
                    let lhs = self.stack.pop().ok_or("stack underflow")?.to_float();
                    if rhs == 0.0 {
                        return Err("float division by zero".to_string());
                    }
                    self.stack.push(Value::Float(FloatVal(lhs / rhs)));
                }
                Instruction::FCmp(op) => {
                    let rhs = self.stack.pop().ok_or("stack underflow")?.to_float();
                    let lhs = self.stack.pop().ok_or("stack underflow")?.to_float();
                    let result = match op {
                        CmpOp::Eq => lhs == rhs,
                        CmpOp::Ne => lhs != rhs,
                        CmpOp::Lt => lhs < rhs,
                        CmpOp::Gt => lhs > rhs,
                        CmpOp::Le => lhs <= rhs,
                        CmpOp::Ge => lhs >= rhs,
                    };
                    self.stack.push(Value::Bool(result));
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
                Instruction::TryEnter(catch_label) => {
                    let start = self.frames[frame_idx].ip;
                    let catch_ip = label_map
                        .get(catch_label.as_str())
                        .copied()
                        .unwrap_or(usize::MAX);
                    let region = TryRegion {
                        start_ip: start,
                        end_ip: catch_ip.saturating_sub(1),
                        catch_label,
                    };
                    self.frames[frame_idx].try_regions.push(region);
                }
                Instruction::TryEnd => {
                    self.frames[frame_idx].try_regions.pop();
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

    /// Call a built-in function. Returns a vec of results pushed onto the stack.
    fn call_builtin(&mut self, name: &str, args: Vec<Value>) -> Result<Vec<Value>, String> {
        let r = match name {
            "print" => {
                for arg in &args {
                    print!("{}", arg.to_string_display());
                }
                println!();
                vec![Value::Nil]
            }
            "print_int" => {
                if let Some(arg) = args.first() {
                    print!("{}", arg.to_int());
                }
                vec![Value::Nil]
            }
            "print_string" => {
                if let Some(arg) = args.first() {
                    print!("{}", arg.to_string_display());
                }
                vec![Value::Nil]
            }
            "to_int" => {
                let n = args.first().map_or(0, |a| a.to_int());
                vec![Value::Int(n)]
            }
            "to_string" => {
                let s = args
                    .first()
                    .map_or(String::new(), |a| a.to_string_display());
                vec![Value::String(s)]
            }
            "len" => {
                let n = if let Some(Value::String(s)) = args.first() {
                    s.len() as i64
                } else {
                    0
                };
                vec![Value::Int(n)]
            }
            "throw_error" => {
                let err_val = args
                    .into_iter()
                    .next()
                    .unwrap_or(Value::String("unhandled exception".to_string()));
                self.error_state = Some(err_val);
                vec![Value::Nil]
            }
            "str_concat" => {
                let mut iter = args.into_iter();
                let a = iter.next().unwrap_or(Value::Nil).to_string_display();
                let b = iter.next().unwrap_or(Value::Nil).to_string_display();
                vec![Value::String(a + &b)]
            }
            "str_eq" => {
                let mut iter = args.into_iter();
                let a = iter.next().unwrap_or(Value::Nil).to_string_display();
                let b = iter.next().unwrap_or(Value::Nil).to_string_display();
                vec![Value::Bool(a == b)]
            }
            "str_contains" => {
                let mut iter = args.into_iter();
                let haystack = iter.next().unwrap_or(Value::Nil).to_string_display();
                let needle = iter.next().unwrap_or(Value::Nil).to_string_display();
                vec![Value::Bool(haystack.contains(&needle))]
            }
            "path_join" => {
                let mut iter = args.into_iter();
                let base = iter.next().unwrap_or(Value::Nil).to_string_display();
                let child = iter.next().unwrap_or(Value::Nil).to_string_display();
                vec![Value::String(
                    std::path::PathBuf::from(base)
                        .join(child)
                        .to_string_lossy()
                        .to_string(),
                )]
            }
            "path_dirname" => {
                let path = args.first().map_or(String::new(), Value::to_string_display);
                let dirname = std::path::Path::new(&path)
                    .parent()
                    .map(|parent| parent.to_string_lossy().to_string())
                    .unwrap_or_default();
                vec![Value::String(dirname)]
            }
            "path_basename" => {
                let path = args.first().map_or(String::new(), Value::to_string_display);
                let basename = std::path::Path::new(&path)
                    .file_name()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                vec![Value::String(basename)]
            }
            "path_extname" => {
                let path = args.first().map_or(String::new(), Value::to_string_display);
                let extname = std::path::Path::new(&path)
                    .extension()
                    .map(|name| name.to_string_lossy().to_string())
                    .unwrap_or_default();
                vec![Value::String(extname)]
            }
            "env_get" => {
                let name = args.first().map_or(String::new(), Value::to_string_display);
                vec![Value::String(std::env::var(name).unwrap_or_default())]
            }
            "env_has" => {
                let name = args.first().map_or(String::new(), Value::to_string_display);
                vec![Value::Bool(std::env::var_os(name).is_some())]
            }
            "read_file" => {
                let path = args.first().map_or(String::new(), Value::to_string_display);
                vec![Value::String(
                    std::fs::read_to_string(path).unwrap_or_default(),
                )]
            }
            "write_file" => {
                let mut iter = args.into_iter();
                let path = iter.next().unwrap_or(Value::Nil).to_string_display();
                let text = iter.next().unwrap_or(Value::Nil).to_string_display();
                vec![Value::Bool(std::fs::write(path, text).is_ok())]
            }
            "array_push" => {
                let mut iter = args.into_iter();
                let arr = iter.next().unwrap_or(Value::Nil);
                let val = iter.next().unwrap_or(Value::Nil);
                if let Value::Array(mut values) = arr {
                    values.push(val);
                    vec![Value::Array(values)]
                } else {
                    vec![Value::Array(vec![val])]
                }
            }
            "array_pop" => {
                let mut iter = args.into_iter();
                let arr = iter.next().unwrap_or(Value::Nil);
                if let Value::Array(mut values) = arr {
                    let popped = values.pop().unwrap_or(Value::Nil);
                    vec![Value::Array(values), popped]
                } else {
                    vec![Value::Array(Vec::new()), Value::Nil]
                }
            }
            "array_len" => {
                let n = match args.first() {
                    Some(Value::Array(values)) => values.len() as i64,
                    _ => 0,
                };
                vec![Value::Int(n)]
            }
            "bool_to_int" => {
                let n = match args.first() {
                    Some(Value::Bool(true)) => 1,
                    _ => 0,
                };
                vec![Value::Int(n)]
            }
            "int_to_bool" => {
                let b = match args.first() {
                    Some(Value::Int(n)) => *n != 0,
                    Some(v) => v.to_bool(),
                    _ => false,
                };
                vec![Value::Bool(b)]
            }
            _ => return Err(format!("unknown builtin: {}", name)),
        };
        Ok(r)
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
        if op == "+" && matches!(lhs, Value::String(_)) && matches!(rhs, Value::String(_)) {
            let left = match lhs {
                Value::String(s) => s,
                _ => unreachable!("checked above"),
            };
            let right = match rhs {
                Value::String(s) => s,
                _ => unreachable!("checked above"),
            };
            return Ok(Value::String(format!("{left}{right}")));
        }
        if matches!(lhs, Value::Float(_)) || matches!(rhs, Value::Float(_)) {
            let l = lhs.to_float();
            let r = rhs.to_float();
            return match op {
                "+" => Ok(Value::Float(FloatVal(l + r))),
                "-" => Ok(Value::Float(FloatVal(l - r))),
                "*" => Ok(Value::Float(FloatVal(l * r))),
                "/" => {
                    if r == 0.0 {
                        Err("float division by zero".to_string())
                    } else {
                        Ok(Value::Float(FloatVal(l / r)))
                    }
                }
                "<" => Ok(Value::Bool(l < r)),
                ">" => Ok(Value::Bool(l > r)),
                "<=" => Ok(Value::Bool(l <= r)),
                ">=" => Ok(Value::Bool(l >= r)),
                _ => Err(format!("unknown float binop: {}", op)),
            };
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
    fn vm_std_path_builtins_execute() {
        let mut module = BytecodeModule::new("main".to_string());
        module.add_function(BytecodeFunction {
            name: "main".to_string(),
            local_count: 0,
            instructions: vec![
                Instruction::LoadString("/tmp".to_string()),
                Instruction::LoadString("demo.in".to_string()),
                Instruction::CallBuiltin("path_join".to_string(), 2),
                Instruction::CallBuiltin("path_basename".to_string(), 1),
                Instruction::Return,
            ],
        });
        let mut vm = BytecodeVM::new(module);
        let result = vm.run().expect("run path builtins");
        assert_eq!(result, Value::String("demo.in".to_string()));
    }

    #[test]
    fn vm_std_env_builtins_execute() {
        let mut module = BytecodeModule::new("main".to_string());
        module.add_function(BytecodeFunction {
            name: "main".to_string(),
            local_count: 0,
            instructions: vec![
                Instruction::LoadString("PATH".to_string()),
                Instruction::CallBuiltin("env_has".to_string(), 1),
                Instruction::Return,
            ],
        });
        let mut vm = BytecodeVM::new(module);
        let result = vm.run().expect("run env builtins");
        assert_eq!(result, Value::Bool(true));
    }

    #[test]
    fn vm_std_fs_builtins_execute() {
        let path = std::env::temp_dir().join("in-stdlib-fs-test.txt");
        let path_text = path.to_string_lossy().to_string();
        let mut module = BytecodeModule::new("main".to_string());
        module.add_function(BytecodeFunction {
            name: "main".to_string(),
            local_count: 0,
            instructions: vec![
                Instruction::LoadString(path_text.clone()),
                Instruction::LoadString("hello compiler".to_string()),
                Instruction::CallBuiltin("write_file".to_string(), 2),
                Instruction::Pop,
                Instruction::LoadString(path_text),
                Instruction::CallBuiltin("read_file".to_string(), 1),
                Instruction::Return,
            ],
        });
        let mut vm = BytecodeVM::new(module);
        let result = vm.run().expect("run fs builtins");
        assert_eq!(result, Value::String("hello compiler".to_string()));
        let _ = std::fs::remove_file(path);
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

    #[test]
    fn vm_try_catch_catches_throw() {
        let mut module = BytecodeModule::new("main".to_string());
        let func = BytecodeFunction {
            name: "main".to_string(),
            instructions: vec![
                Instruction::TryEnter("catch_0".to_string()),
                Instruction::LoadString("boom".to_string()),
                Instruction::CallBuiltin("throw_error".to_string(), 1),
                Instruction::TryEnd,
                Instruction::Jump("end_0".to_string()),
                Instruction::Label("catch_0".to_string()),
                Instruction::LoadInt(42),
                Instruction::Label("end_0".to_string()),
                Instruction::Return,
            ],
            local_count: 0,
        };
        module.add_function(func);
        let mut vm = BytecodeVM::new(module);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(42));
    }

    #[test]
    fn vm_uncaught_throw_produces_error() {
        let mut module = BytecodeModule::new("main".to_string());
        let func = BytecodeFunction {
            name: "main".to_string(),
            instructions: vec![
                Instruction::LoadString("unhandled".to_string()),
                Instruction::CallBuiltin("throw_error".to_string(), 1),
                Instruction::LoadInt(99),
                Instruction::Return,
            ],
            local_count: 0,
        };
        module.add_function(func);
        let mut vm = BytecodeVM::new(module);
        assert!(vm.run().is_err());
    }

    #[test]
    fn vm_throw_inside_nested_try_caught_by_outer() {
        let mut module = BytecodeModule::new("main".to_string());
        let func = BytecodeFunction {
            name: "main".to_string(),
            instructions: vec![
                Instruction::TryEnter("catch_outer".to_string()),
                Instruction::LoadInt(1),
                Instruction::Store(0),
                Instruction::TryEnter("catch_inner".to_string()),
                Instruction::LoadString("nested".to_string()),
                Instruction::CallBuiltin("throw_error".to_string(), 1),
                Instruction::TryEnd,
                Instruction::Jump("end_inner".to_string()),
                Instruction::Label("catch_inner".to_string()),
                Instruction::LoadString("rethrow".to_string()),
                Instruction::CallBuiltin("throw_error".to_string(), 1),
                Instruction::Label("end_inner".to_string()),
                Instruction::TryEnd,
                Instruction::Jump("end_outer".to_string()),
                Instruction::Label("catch_outer".to_string()),
                Instruction::LoadInt(7),
                Instruction::Store(0),
                Instruction::Label("end_outer".to_string()),
                Instruction::Load(0),
                Instruction::Return,
            ],
            local_count: 1,
        };
        module.add_function(func);
        let mut vm = BytecodeVM::new(module);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Int(7));
    }

    #[test]
    fn vm_float_add() {
        let mut module = BytecodeModule::new("main".to_string());
        let func = BytecodeFunction {
            name: "main".to_string(),
            instructions: vec![
                Instruction::LoadFloat(FloatVal(1.5)),
                Instruction::LoadFloat(FloatVal(2.5)),
                Instruction::FAdd,
                Instruction::Return,
            ],
            local_count: 0,
        };
        module.add_function(func);
        let mut vm = BytecodeVM::new(module);
        let result = vm.run().unwrap();
        assert_eq!(result, Value::Float(FloatVal(4.0)));
    }
}
