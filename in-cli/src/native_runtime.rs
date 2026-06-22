use crate::bytecode::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Native function type: takes args, returns Value or error.
pub type NativeFn = Box<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;

/// Registry of native function implementations for crate/stdlib calls.
pub struct NativeRuntime {
    fns: BTreeMap<String, NativeFn>,
}

impl NativeRuntime {
    pub fn new() -> Self {
        Self { fns: BTreeMap::new() }
    }

    /// Register a native function under the given name.
    pub fn register(&mut self, name: &str, f: NativeFn) {
        self.fns.insert(name.to_string(), f);
    }

    /// Try to call a native function. Returns None if not found.
    pub fn call(&self, name: &str, args: &[Value]) -> Option<Result<Value, String>> {
        self.fns.get(name).map(|f| f(args))
    }

    /// Build a standard runtime with common stdlib/crate functions.
    pub fn standard() -> Self {
        let mut rt = Self::new();

        // std::env::current_dir / cwd
        rt.register("cwd", Box::new(|_args| {
            match std::env::current_dir() {
                Ok(path) => Ok(Value::String(path.display().to_string())),
                Err(e) => Ok(Value::Nil), // ponytail: return Nil on error
            }
        }));

        // PathBuf operations
        rt.register("display", Box::new(|args| {
            if let Some(Value::String(s)) = args.first() {
                Ok(Value::String(s.clone()))
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("join", Box::new(|args| {
            if args.len() >= 2 {
                let a = value_to_string(&args[0]);
                let b = value_to_string(&args[1]);
                let path = PathBuf::from(&a).join(&b);
                Ok(Value::String(path.display().to_string()))
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("parent", Box::new(|args| {
            if let Some(Value::String(s)) = args.first() {
                let path = PathBuf::from(s);
                if let Some(parent) = path.parent() {
                    Ok(Value::String(parent.display().to_string()))
                } else {
                    Ok(Value::Nil)
                }
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("is_dir", Box::new(|args| {
            if let Some(Value::String(s)) = args.first() {
                Ok(Value::Bool(PathBuf::from(s).is_dir()))
            } else {
                Ok(Value::Bool(false))
            }
        }));

        rt.register("exists", Box::new(|args| {
            if let Some(Value::String(s)) = args.first() {
                Ok(Value::Bool(PathBuf::from(s).exists()))
            } else {
                Ok(Value::Bool(false))
            }
        }));

        rt.register("extension", Box::new(|args| {
            if let Some(Value::String(s)) = args.first() {
                let ext = PathBuf::from(s)
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(|s| s.to_string());
                match ext {
                    Some(e) => Ok(Value::String(e)),
                    None => Ok(Value::Nil),
                }
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("file_stem", Box::new(|args| {
            if let Some(Value::String(s)) = args.first() {
                let stem = PathBuf::from(s)
                    .file_stem()
                    .and_then(|s| s.to_str())
                    .map(|s| s.to_string());
                match stem {
                    Some(s) => Ok(Value::String(s)),
                    None => Ok(Value::Nil),
                }
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("to_string_lossy", Box::new(|args| {
            if let Some(Value::String(s)) = args.first() {
                Ok(Value::String(s.clone()))
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("to_string", Box::new(|args| {
            Ok(Value::String(args.iter().map(value_to_string).collect::<Vec<_>>().join("")))
        }));

        rt.register("clone", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("as_deref", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("unwrap_or_default", Box::new(|args| {
            match args.first() {
                Some(Value::Nil) => Ok(Value::String(String::new())),
                Some(v) => Ok(v.clone()),
                None => Ok(Value::Nil),
            }
        }));

        rt.register("map_err", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("unwrap_or", Box::new(|args| {
            if let Some(v) = args.first() {
                if matches!(v, Value::Nil) {
                    args.get(1).cloned().map(Ok).unwrap_or(Ok(Value::Nil))
                } else {
                    Ok(v.clone())
                }
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("map", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("collect", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("iter", Box::new(|args| {
            // Return the value itself for iteration
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("is_ok", Box::new(|args| {
            Ok(Value::Bool(!matches!(args.first(), Some(Value::Nil))))
        }));

        rt.register("is_err", Box::new(|args| {
            Ok(Value::Bool(matches!(args.first(), Some(Value::Nil))))
        }));

        rt.register("is_some_and", Box::new(|args| {
            Ok(Value::Bool(!matches!(args.first(), Some(Value::Nil))))
        }));

        rt.register("any", Box::new(|args| {
            Ok(Value::Bool(false))
        }));

        rt.register("is_empty", Box::new(|args| {
            Ok(Value::Bool(matches!(args.first(), Some(Value::Array(a)) if a.is_empty())))
        }));

        rt.register("ends_with", Box::new(|args| {
            if let (Some(Value::String(s)), Some(Value::String(suffix))) = (args.first(), args.get(1)) {
                Ok(Value::Bool(s.ends_with(suffix)))
            } else {
                Ok(Value::Bool(false))
            }
        }));

        rt.register("unwrap", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("len", Box::new(|args| {
            match args.first() {
                Some(Value::String(s)) => Ok(Value::Int(s.len() as i64)),
                Some(Value::Array(a)) => Ok(Value::Int(a.len() as i64)),
                _ => Ok(Value::Int(0)),
            }
        }));

        // serde_json
        rt.register("serde_json :: to_string_pretty", Box::new(|args| {
            // ponytail: return JSON string representation
            Ok(Value::String(format!("{:?}", args)))
        }));

        rt.register("serde_json :: to_string", Box::new(|args| {
            Ok(Value::String(format!("{:?}", args)))
        }));

        // clap - basic stubs that return reasonable defaults
        rt.register("Cli :: parse", Box::new(|_args| {
            // Return a mock Command with subcommands
            Ok(Value::Nil) // ponytail: real CLI parsing would need the actual binary
        }));

        // Instant::now
        rt.register("Instant :: now", Box::new(|_args| {
            Ok(Value::Int(0)) // ponytail: return 0 for timing
        }));

        rt.register("elapsed", Box::new(|args| {
            // Return Duration
            Ok(Value::Int(0))
        }));

        rt.register("as_secs_f64", Box::new(|args| {
            Ok(Value::String("0.000".to_string()))
        }));

        // String operations
        rt.register("String :: new", Box::new(|_args| {
            Ok(Value::String(String::new()))
        }));

        rt.register("std :: fs :: read_to_string", Box::new(|args| {
            if let Some(Value::String(path)) = args.first() {
                match std::fs::read_to_string(path) {
                    Ok(content) => Ok(Value::String(content)),
                    Err(_) => Ok(Value::Nil),
                }
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("std :: env :: temp_dir", Box::new(|_args| {
            Ok(Value::String(std::env::temp_dir().display().to_string()))
        }));

        rt.register("std :: env :: var", Box::new(|args| {
            if let Some(Value::String(key)) = args.first() {
                match std::env::var(key) {
                    Ok(val) => Ok(Value::String(val)),
                    Err(_) => Ok(Value::Nil),
                }
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("std :: process :: exit", Box::new(|_args| {
            // Don't actually exit — just return Nil
            Ok(Value::Nil)
        }));

        // format! macro equivalent
        rt.register("format", Box::new(|args| {
            Ok(Value::String(args.iter().map(value_to_string).collect::<Vec<_>>().join("")))
        }));

        // fs operations
        rt.register("fs :: create_dir_all", Box::new(|args| {
            if let Some(Value::String(path)) = args.first() {
                let _ = std::fs::create_dir_all(path);
            }
            Ok(Value::Nil)
        }));

        rt.register("fs :: metadata", Box::new(|args| {
            if let Some(Value::String(path)) = args.first() {
                match std::fs::metadata(path) {
                    Ok(_) => Ok(Value::Bool(true)),
                    Err(_) => Ok(Value::Nil),
                }
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("fs :: read_to_string", Box::new(|args| {
            if let Some(Value::String(path)) = args.first() {
                match std::fs::read_to_string(path) {
                    Ok(content) => Ok(Value::String(content)),
                    Err(_) => Ok(Value::Nil),
                }
            } else {
                Ok(Value::Nil)
            }
        }));

        // Command execution
        rt.register("Command :: new", Box::new(|args| {
            if let Some(Value::String(prog)) = args.first() {
                Ok(Value::String(prog.clone()))
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("arg", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("current_dir", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("status", Box::new(|_args| {
            Ok(Value::Bool(true))
        }));

        rt.register("success", Box::new(|_args| {
            Ok(Value::Bool(true))
        }));

        rt.register("output", Box::new(|_args| {
            Ok(Value::Nil)
        }));

        // Runtime
        rt.register("build", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("enable_all", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("block_on", Box::new(|args| {
            // Return Nil — async execution not needed for self-hosting
            Ok(Value::Nil)
        }));

        rt.register("spawn", Box::new(|args| {
            Ok(Value::Nil)
        }));

        rt.register("sleep", Box::new(|_args| {
            Ok(Value::Nil)
        }));

        rt.register("await", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("abort", Box::new(|_args| {
            Ok(Value::Nil)
        }));

        // Inauguration internal functions
        rt.register("resolve_invocation_path", Box::new(|args| {
            // Return cwd joined with path
            if args.len() >= 2 {
                let cwd = value_to_string(&args[0]);
                let path = value_to_string(&args[1]);
                let resolved = PathBuf::from(&cwd).join(&path);
                Ok(Value::String(resolved.display().to_string()))
            } else {
                Ok(Value::Nil)
            }
        }));

        rt.register("workspace_root", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        // Type conversions
        rt.register("into", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        rt.register("as_str", Box::new(|args| {
            args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
        }));

        // serde
        rt.register("serde :: Serialize", Box::new(|args| {
            Ok(Value::String("serialized".to_string()))
        }));

        rt.register("serde :: Deserialize", Box::new(|args| {
            Ok(Value::Nil)
        }));

        // thiserror
        rt.register("thiserror :: Error", Box::new(|args| {
            Ok(Value::String("error".to_string()))
        }));

        rt
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Int(i) => i.to_string(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Nil => String::new(),
        Value::Array(arr) => format!("[{}]", arr.iter().map(value_to_string).collect::<Vec<_>>().join(", ")),
        _ => format!("{:?}", v),
    }
}
