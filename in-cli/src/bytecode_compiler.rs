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
    pub identity: crate::core_ir::ModuleIdentityReport,
}

pub fn compile_source_path(
    path: &Path,
    module_id: &str,
    parser: ParserCli,
) -> Result<BytecodeCompileOutput, String> {
    let resolved = parser_registry::resolve_parser_id(path, parser);
    let Some(mut module) =
        parser_registry::parse_with_resolved(resolved, path).map_err(|err| err.to_string())?
    else {
        return Err("bytecode compiler requires a Core IR frontend; Swift SIL emit is not supported by this path".to_string());
    };
    // Desugar classes to structs before typecheck
    crate::lower_core::desugar_module(&mut module);
    core_typecheck::typecheck_executable(&module)?;

    if std::env::var("IN_TYPECHECK").is_ok() {
        let strict = std::env::var("IN_TYPECHECK").as_deref() == Ok("strict");
        if let Err(errors) = crate::typecheck::TypeChecker::new().check_module(&module) {
            for err in &errors {
                eprintln!("[typecheck] {:?}", err);
            }
            if strict {
                return Err("typecheck-failed".to_string());
            }
        }
    }

    let identity = module.identity_report(module_id);
    let sil = crate::compiler::driver::lower_unified_module(&module, &identity.effective_module_id);
    let artifact = crate::hybrid_sil::parse_textual_sil(&sil);
    let mut module = sil_to_bytecode::lower_sil_to_bytecode(&artifact)?;
    module.identity = Some(identity.clone());
    module.package_exports = crate::package_runtime::collect_package_exports_for_source(path);
    Ok(BytecodeCompileOutput {
        module,
        sil,
        identity,
    })
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
    fn compile_output_and_artifact_carry_core_identity_metadata() {
        let source_path = temp_file("identity.in");
        let bytecode_path = temp_file("identity.bca");
        fs::write(
            &source_path,
            "package agents.video;\nmodule agents.video.main;\nfn helper(value: Int) -> Int { return value; }\nfn main() -> Int { return helper(9); }\n",
        )
        .unwrap();

        let output = compile_source_path(&source_path, "App", ParserCli::Auto).unwrap();
        assert_eq!(output.identity.package.as_deref(), Some("agents.video"));
        assert_eq!(output.identity.module.as_deref(), Some("agents.video.main"));
        assert_eq!(output.identity.requested_module_id, "App");
        assert_eq!(output.identity.effective_module_id, "agents.video.main");
        let module_identity = output.module.identity.as_ref().expect("module identity");
        assert_eq!(module_identity.package.as_deref(), Some("agents.video"));
        assert_eq!(module_identity.module.as_deref(), Some("agents.video.main"));
        assert_eq!(module_identity.requested_module_id, "App");
        assert_eq!(module_identity.effective_module_id, "agents.video.main");
        assert!(output.sil.contains("sil @helper"), "{}", output.sil);
        assert!(
            !output.sil.contains("sil @agents.video.main.helper"),
            "{}",
            output.sil
        );
        assert_eq!(
            run_bytecode_module(output.module.clone()).unwrap(),
            Value::Int(9)
        );

        write_bytecode_module(&output.module, &bytecode_path).unwrap();
        let roundtrip = read_bytecode_module(&bytecode_path).unwrap();
        assert_eq!(roundtrip.entry_point, "main");
        let roundtrip_identity = roundtrip.identity.as_ref().expect("roundtrip identity");
        assert_eq!(roundtrip_identity.package.as_deref(), Some("agents.video"));
        assert_eq!(
            roundtrip_identity.module.as_deref(),
            Some("agents.video.main")
        );
        assert_eq!(roundtrip_identity.requested_module_id.as_str(), "App");
        assert_eq!(
            roundtrip_identity.effective_module_id.as_str(),
            "agents.video.main"
        );
        assert_eq!(run_bytecode_module(roundtrip).unwrap(), Value::Int(9));

        fs::remove_file(source_path).unwrap();
        fs::remove_file(bytecode_path).unwrap();
    }

    #[test]
    fn compiles_in_modulo_to_runnable_bytecode() {
        let path = temp_file("modulo.in");
        fs::write(&path, "fn main() -> Int { return 7 % 4; }\n").unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(3));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn rejects_unresolved_identifier_before_lowering() {
        let path = temp_file("unresolved-ident.in");
        fs::write(&path, "fn main() -> Int { return missing; }\n").unwrap();

        let err = compile_source_path(&path, "App", ParserCli::Auto)
            .expect_err("unresolved identifiers should fail before bytecode lowering");
        assert!(
            err.contains("unresolved identifier `missing` in `main`"),
            "unexpected error: {err}"
        );

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
    fn bytecode_artifact_preserves_string_literals_with_spaces() {
        let source_path = temp_file("string-space.in");
        let bytecode_path = temp_file("string-space.bca");
        fs::write(
            &source_path,
            "fn main() -> String { return \"hello world\"; }\n",
        )
        .unwrap();

        let output = compile_source_path(&source_path, "App", ParserCli::Auto).unwrap();
        assert_eq!(
            run_bytecode_module(output.module.clone()).unwrap(),
            Value::String("hello world".to_string())
        );
        write_bytecode_module(&output.module, &bytecode_path).unwrap();
        let roundtrip = read_bytecode_module(&bytecode_path).unwrap();
        assert_eq!(
            run_bytecode_module(roundtrip).unwrap(),
            Value::String("hello world".to_string())
        );

        fs::remove_file(source_path).unwrap();
        fs::remove_file(bytecode_path).unwrap();
    }

    #[test]
    fn compiles_array_literal_and_index_access() {
        let path = temp_file("array-index.in");
        fs::write(
            &path,
            "fn main() -> Int { let xs: [Int] = [2, 5, 8]; return xs[1]; }\n",
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(5));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn compiles_array_index_assignment() {
        let path = temp_file("array-index-assign.in");
        fs::write(
            &path,
            "fn main() -> Int { let xs: [Int] = [2, 5, 8]; xs[1] = 9; return xs[1]; }\n",
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(9));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn compiles_struct_method_call() {
        let path = temp_file("method-call.in");
        fs::write(
            &path,
            r#"
struct Point {
  Int x
  Int y

  fn sum() -> Int {
    return self.x + self.y;
  }
}

fn main() -> Int {
  let p: Point = Point { x: 2, y: 5 };
  return p.sum();
}
"#,
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(7));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn branch_assignment_updates_variable_value() {
        let path = temp_file("branch-assign.in");
        fs::write(
            &path,
            "fn main() -> Int { let x: Int = 0; if true { x = 9; } return x; }\n",
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(9));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn while_loop_updates_variable_until_condition_is_false() {
        let path = temp_file("while-loop.in");
        fs::write(
            &path,
            "fn main() -> Int { let x: Int = 0; while x < 3 { x = x + 1; } return x; }\n",
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(3));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn while_loop_skips_body_when_condition_is_false() {
        let path = temp_file("while-skip.in");
        fs::write(
            &path,
            "fn main() -> Int { let x: Int = 5; while x < 3 { x = x + 1; } return x; }\n",
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(5));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn match_statement_selects_single_arm() {
        let path = temp_file("match.in");
        fs::write(
            &path,
            r#"
fn choose(tag: Int) -> Int {
  let out: Int = 0;
  match tag {
    1 { out = 10; }
    _ { out = 20; }
  }
  return out;
}
fn main() -> Int { return choose(1); }
"#,
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(10));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn string_equality_compares_string_values() {
        let path = temp_file("string-eq.in");
        fs::write(
            &path,
            "fn main() -> Bool { return \"left\" == \"right\"; }\n",
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(
            run_bytecode_module(output.module).unwrap(),
            Value::Bool(false)
        );

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn struct_field_access_returns_scalar() {
        let path = temp_file("struct-field.in");
        fs::write(
            &path,
            "struct Point { Int x; Int y }\nfn main() -> Int { let p: Point = Point { x: 2, y: 5 }; return p.y; }\n",
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(5));

        fs::remove_file(path).unwrap();
    }

    #[test]
    fn direct_struct_field_access_returns_scalar() {
        let path = temp_file("direct-struct-field.in");
        fs::write(
            &path,
            "struct Point { Int x; Int y }\nfn main() -> Int { return Point { x: 2, y: 5 }.y; }\n",
        )
        .unwrap();

        let output = compile_source_path(&path, "App", ParserCli::Auto).unwrap();
        assert_eq!(run_bytecode_module(output.module).unwrap(), Value::Int(5));

        fs::remove_file(path).unwrap();
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
