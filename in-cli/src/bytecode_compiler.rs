use crate::bytecode::{BytecodeModule, Value, module_to_text, text_to_module};
use crate::core_typecheck;
use crate::parser_registry::{self, ParserCli};
use crate::sil_to_bytecode;
use crate::vm::BytecodeVM;
use std::fs;
use std::path::Path;

#[derive(Debug, Clone)]
pub struct BytecodeCompileOutput {
    pub module: BytecodeModule,
    pub sil: String,
}

pub fn compile_source_path(
    path: &Path,
    module_id: &str,
    parser: ParserCli,
) -> Result<BytecodeCompileOutput, String> {
    let resolved = parser_registry::resolve_parser_id(path, parser);
    let Some(module) =
        parser_registry::parse_with_resolved(resolved, path).map_err(|err| err.to_string())?
    else {
        return Err("bytecode compiler requires a Core IR frontend; Swift SIL emit is not supported by this path".to_string());
    };
    core_typecheck::typecheck_executable(&module)?;
    let sil = crate::compiler::driver::lower_unified_module(&module, module_id);
    let artifact = crate::hybrid_sil::parse_textual_sil(&sil);
    let module = sil_to_bytecode::lower_sil_to_bytecode(&artifact)?;
    Ok(BytecodeCompileOutput { module, sil })
}

pub fn write_bytecode_module(module: &BytecodeModule, out_path: &Path) -> Result<(), String> {
    if let Some(parent) = out_path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|err| format!("create output dir: {err}"))?;
    }
    fs::write(out_path, module_to_text(module)).map_err(|err| format!("write bytecode: {err}"))
}

pub fn read_bytecode_module(path: &Path) -> Result<BytecodeModule, String> {
    let text = fs::read_to_string(path).map_err(|err| format!("read bytecode: {err}"))?;
    text_to_module(&text).map_err(|err| format!("parse bytecode: {err}"))
}

pub fn run_bytecode_module(module: BytecodeModule) -> Result<Value, String> {
    let mut vm = BytecodeVM::new(module);
    vm.run()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn temp_file(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "inauguration-bytecode-{}-{}-{name}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ))
    }

    #[test]
    fn compiles_in_source_to_runnable_bytecode() {
        let path = temp_file("main.in");
        fs::write(
            &path,
            "fn helper(value: Int) -> Int { return value; }\nfn main() -> Int { return helper(7); }\n",
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert!(
            output.sil.contains("function_ref @helper"),
            "{}",
            output.sil
        );
        assert!(
            output
                .module
                .functions
                .iter()
                .any(|function| function.name == "main")
        );
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(7));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn writes_and_reads_bytecode_artifact() {
        let source_path = temp_file("artifact.in");
        let bytecode_path = temp_file("artifact.bca");
        fs::write(&source_path, "fn main() -> void {}\n").unwrap();

        let output = compile_source_path(&source_path, "App", ParserCli::Auto).unwrap();
        write_bytecode_module(&output.module, &bytecode_path).unwrap();
        let roundtrip = read_bytecode_module(&bytecode_path).unwrap();
        assert_eq!(roundtrip.entry_point, output.module.entry_point);
        run_bytecode_module(roundtrip).unwrap();

        fs::remove_file(source_path).unwrap();
        fs::remove_file(bytecode_path).unwrap();
    }

    #[test]
    fn runs_string_bool_and_if_return_bytecode() {
        let path = temp_file("agent.in");
        fs::write(
            &path,
            "fn ready(flag: Bool) -> String { if flag { return \"ready\"; } return \"no\"; }\nfn main() -> String { return ready(true); }\n",
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(
            run_bytecode_module(output.module).unwrap(),
            Value::String("ready".to_string())
        );

        fs::remove_file(path).unwrap();
    }
}
