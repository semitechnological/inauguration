use crate::bytecode::Value;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Native function type: takes args, returns Value or error.
pub type NativeFn = Box<dyn Fn(&[Value]) -> Result<Value, String> + Send + Sync>;

/// Registry of native function implementations for crate/stdlib calls.
pub struct NativeRuntime {
    fns: BTreeMap<String, NativeFn>,
}

impl Default for NativeRuntime {
    fn default() -> Self {
        Self::new()
    }
}

impl NativeRuntime {
    pub fn new() -> Self {
        Self {
            fns: BTreeMap::new(),
        }
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
        rt.register_std_env_current_dir_cwd();
        rt.register_pathbuf_operations();
        rt.register_serde_json();
        rt.register_clap_basic_stubs_that_return_reasonable_defaults();
        rt.register_instant_now();
        rt.register_string_operations();
        rt.register_format_macro_equivalent();
        rt.register_fs_operations();
        rt.register_command_execution();
        rt.register_runtime();
        rt.register_inauguration_internal_functions();
        rt.register_type_conversions();
        rt.register_serde();
        rt.register_thiserror();

        rt
    }

    fn register_std_env_current_dir_cwd(&mut self) {
        // std::env::current_dir / cwd
        self.register(
            "cwd",
            Box::new(|_args| {
                match std::env::current_dir() {
                    Ok(path) => Ok(Value::String(path.display().to_string())),
                    Err(_e) => Ok(Value::Nil), // ponytail: return Nil on error
                }
            }),
        );
    }

    fn register_pathbuf_operations(&mut self) {
        // PathBuf operations
        self.register(
            "display",
            Box::new(|args| {
                if let Some(Value::String(s)) = args.first() {
                    Ok(Value::String(s.clone()))
                } else {
                    Ok(Value::Nil)
                }
            }),
        );

        self.register(
            "join",
            Box::new(|args| {
                if args.len() >= 2 {
                    let a = value_to_string(&args[0]);
                    let b = value_to_string(&args[1]);
                    let path = PathBuf::from(&a).join(&b);
                    Ok(Value::String(path.display().to_string()))
                } else {
                    Ok(Value::Nil)
                }
            }),
        );

        self.register(
            "parent",
            Box::new(|args| {
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
            }),
        );

        self.register(
            "is_dir",
            Box::new(|args| {
                if let Some(Value::String(s)) = args.first() {
                    Ok(Value::Bool(PathBuf::from(s).is_dir()))
                } else {
                    Ok(Value::Bool(false))
                }
            }),
        );

        self.register(
            "exists",
            Box::new(|args| {
                if let Some(Value::String(s)) = args.first() {
                    Ok(Value::Bool(PathBuf::from(s).exists()))
                } else {
                    Ok(Value::Bool(false))
                }
            }),
        );

        self.register(
            "extension",
            Box::new(|args| {
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
            }),
        );

        self.register(
            "file_stem",
            Box::new(|args| {
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
            }),
        );

        self.register(
            "to_string_lossy",
            Box::new(|args| {
                if let Some(Value::String(s)) = args.first() {
                    Ok(Value::String(s.clone()))
                } else {
                    Ok(Value::Nil)
                }
            }),
        );

        self.register(
            "to_string",
            Box::new(|args| {
                Ok(Value::String(
                    args.iter()
                        .map(value_to_string)
                        .collect::<Vec<_>>()
                        .join(""),
                ))
            }),
        );

        self.register(
            "clone",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register(
            "as_deref",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register(
            "unwrap_or_default",
            Box::new(|args| match args.first() {
                Some(Value::Nil) => Ok(Value::String(String::new())),
                Some(v) => Ok(v.clone()),
                None => Ok(Value::Nil),
            }),
        );

        self.register(
            "map_err",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register(
            "unwrap_or",
            Box::new(|args| {
                if let Some(v) = args.first() {
                    if matches!(v, Value::Nil) {
                        args.get(1).cloned().map(Ok).unwrap_or(Ok(Value::Nil))
                    } else {
                        Ok(v.clone())
                    }
                } else {
                    Ok(Value::Nil)
                }
            }),
        );

        self.register(
            "map",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register(
            "collect",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register(
            "iter",
            Box::new(|args| {
                // Return the value itself for iteration
                args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))
            }),
        );

        self.register(
            "is_ok",
            Box::new(|args| Ok(Value::Bool(!matches!(args.first(), Some(Value::Nil))))),
        );

        self.register(
            "is_err",
            Box::new(|args| Ok(Value::Bool(matches!(args.first(), Some(Value::Nil))))),
        );

        self.register(
            "is_some_and",
            Box::new(|args| Ok(Value::Bool(!matches!(args.first(), Some(Value::Nil))))),
        );

        self.register("any", Box::new(|_args| Ok(Value::Bool(false))));

        self.register(
            "is_empty",
            Box::new(|args| {
                Ok(Value::Bool(
                    matches!(args.first(), Some(Value::Array(a)) if a.is_empty()),
                ))
            }),
        );

        self.register(
            "ends_with",
            Box::new(|args| {
                if let (Some(Value::String(s)), Some(Value::String(suffix))) =
                    (args.first(), args.get(1))
                {
                    Ok(Value::Bool(s.ends_with(suffix)))
                } else {
                    Ok(Value::Bool(false))
                }
            }),
        );

        self.register(
            "unwrap",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register(
            "len",
            Box::new(|args| match args.first() {
                Some(Value::String(s)) => Ok(Value::Int(s.len() as i64)),
                Some(Value::Array(a)) => Ok(Value::Int(a.len() as i64)),
                _ => Ok(Value::Int(0)),
            }),
        );
    }

    fn register_serde_json(&mut self) {
        // serde_json
        self.register(
            "serde_json :: to_string_pretty",
            Box::new(|args| {
                // ponytail: return JSON string representation
                Ok(Value::String(format!("{:?}", args)))
            }),
        );

        self.register(
            "serde_json :: to_string",
            Box::new(|args| Ok(Value::String(format!("{:?}", args)))),
        );
    }

    fn register_clap_basic_stubs_that_return_reasonable_defaults(&mut self) {
        // clap - basic stubs that return reasonable defaults
        self.register(
            "Cli :: parse",
            Box::new(|_args| {
                // Return a mock Command with subcommands
                Ok(Value::Nil) // ponytail: real CLI parsing would need the actual binary
            }),
        );
    }

    fn register_instant_now(&mut self) {
        // Instant::now
        self.register(
            "Instant :: now",
            Box::new(|_args| {
                Ok(Value::Int(0)) // ponytail: return 0 for timing
            }),
        );

        self.register(
            "elapsed",
            Box::new(|_args| {
                // Return Duration
                Ok(Value::Int(0))
            }),
        );

        self.register(
            "as_secs_f64",
            Box::new(|_args| Ok(Value::String("0.000".to_string()))),
        );
    }

    fn register_string_operations(&mut self) {
        // String operations
        self.register(
            "String :: new",
            Box::new(|_args| Ok(Value::String(String::new()))),
        );

        self.register(
            "std :: fs :: read_to_string",
            Box::new(|args| {
                if let Some(Value::String(path)) = args.first() {
                    match std::fs::read_to_string(path) {
                        Ok(content) => Ok(Value::String(content)),
                        Err(_) => Ok(Value::Nil),
                    }
                } else {
                    Ok(Value::Nil)
                }
            }),
        );

        self.register(
            "std :: env :: temp_dir",
            Box::new(|_args| Ok(Value::String(std::env::temp_dir().display().to_string()))),
        );

        self.register(
            "std :: env :: var",
            Box::new(|args| {
                if let Some(Value::String(key)) = args.first() {
                    match std::env::var(key) {
                        Ok(val) => Ok(Value::String(val)),
                        Err(_) => Ok(Value::Nil),
                    }
                } else {
                    Ok(Value::Nil)
                }
            }),
        );

        self.register(
            "std :: process :: exit",
            Box::new(|_args| {
                // Don't actually exit — just return Nil
                Ok(Value::Nil)
            }),
        );
    }

    fn register_format_macro_equivalent(&mut self) {
        // format! macro equivalent
        self.register(
            "format",
            Box::new(|args| {
                Ok(Value::String(
                    args.iter()
                        .map(value_to_string)
                        .collect::<Vec<_>>()
                        .join(""),
                ))
            }),
        );
    }

    fn register_fs_operations(&mut self) {
        // fs operations
        self.register(
            "fs :: create_dir_all",
            Box::new(|args| {
                if let Some(Value::String(path)) = args.first() {
                    let _ = std::fs::create_dir_all(path);
                }
                Ok(Value::Nil)
            }),
        );

        self.register(
            "fs :: metadata",
            Box::new(|args| {
                if let Some(Value::String(path)) = args.first() {
                    match std::fs::metadata(path) {
                        Ok(_) => Ok(Value::Bool(true)),
                        Err(_) => Ok(Value::Nil),
                    }
                } else {
                    Ok(Value::Nil)
                }
            }),
        );

        self.register(
            "fs :: read_to_string",
            Box::new(|args| {
                if let Some(Value::String(path)) = args.first() {
                    match std::fs::read_to_string(path) {
                        Ok(content) => Ok(Value::String(content)),
                        Err(_) => Ok(Value::Nil),
                    }
                } else {
                    Ok(Value::Nil)
                }
            }),
        );
    }

    fn register_command_execution(&mut self) {
        // Command execution
        self.register(
            "Command :: new",
            Box::new(|args| {
                if let Some(Value::String(prog)) = args.first() {
                    Ok(Value::String(prog.clone()))
                } else {
                    Ok(Value::Nil)
                }
            }),
        );

        self.register(
            "arg",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register(
            "current_dir",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register("status", Box::new(|_args| Ok(Value::Bool(true))));

        self.register("success", Box::new(|_args| Ok(Value::Bool(true))));

        self.register("output", Box::new(|_args| Ok(Value::Nil)));
    }

    fn register_runtime(&mut self) {
        // Runtime
        self.register(
            "build",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register(
            "enable_all",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register(
            "block_on",
            Box::new(|_args| {
                // Return Nil — async execution not needed for self-hosting
                Ok(Value::Nil)
            }),
        );

        self.register("spawn", Box::new(|_args| Ok(Value::Nil)));

        self.register("sleep", Box::new(|_args| Ok(Value::Nil)));

        self.register(
            "await",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register("abort", Box::new(|_args| Ok(Value::Nil)));
    }

    fn register_inauguration_internal_functions(&mut self) {
        // Inauguration internal functions
        self.register(
            "resolve_invocation_path",
            Box::new(|args| {
                // Return cwd joined with path
                if args.len() >= 2 {
                    let cwd = value_to_string(&args[0]);
                    let path = value_to_string(&args[1]);
                    let resolved = PathBuf::from(&cwd).join(&path);
                    Ok(Value::String(resolved.display().to_string()))
                } else {
                    Ok(Value::Nil)
                }
            }),
        );

        self.register(
            "workspace_root",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );
    }

    fn register_type_conversions(&mut self) {
        // Type conversions
        self.register(
            "into",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );

        self.register(
            "as_str",
            Box::new(|args| args.first().cloned().map(Ok).unwrap_or(Ok(Value::Nil))),
        );
    }

    fn register_serde(&mut self) {
        // serde
        self.register(
            "serde :: Serialize",
            Box::new(|_args| Ok(Value::String("serialized".to_string()))),
        );

        self.register("serde :: Deserialize", Box::new(|_args| Ok(Value::Nil)));
    }

    fn register_thiserror(&mut self) {
        // thiserror
        self.register(
            "thiserror :: Error",
            Box::new(|_args| Ok(Value::String("error".to_string()))),
        );
    }
}

fn value_to_string(v: &Value) -> String {
    match v {
        Value::Int(i) => i.to_string(),
        Value::String(s) => s.clone(),
        Value::Bool(b) => b.to_string(),
        Value::Nil => String::new(),
        Value::Array(arr) => format!(
            "[{}]",
            arr.iter()
                .map(value_to_string)
                .collect::<Vec<_>>()
                .join(", ")
        ),
        _ => format!("{:?}", v),
    }
}
