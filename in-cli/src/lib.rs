//! Library crate backing the `in` CLI — hybrid compiler wave plus embedded hotreload daemon.

#[cfg(unix)]
pub mod preview_client;

pub mod agent_mode;
pub mod boundary_ir;
pub mod boundary_verify;
pub mod bytecode;
pub mod bytecode_compiler;
pub mod compile_cache;
pub mod compiler;
pub mod core_ir;
pub mod core_ir_verifier;
pub mod core_typecheck;
pub mod extension_registry;
pub mod typecheck;
pub mod external_guard;
pub mod graph_report;
pub mod hotreload;
pub mod hybrid_core;
pub mod hybrid_pipeline;
pub mod hybrid_scheduler;
pub mod hybrid_sil;
pub mod in_canonical;
pub mod in_lang_parse;
pub mod inrt;
pub mod language_support;
pub mod lower_core;
pub mod mobile;
pub mod module_resolver;
pub mod native_backend;
pub mod native_emit;
pub mod native_swift_sil;
pub mod owned_compile;
pub mod package_manifest;
pub mod parser_registry;
pub mod sil_emit;
pub mod sil_to_bytecode;
pub mod swift_subset;
pub mod v_native;
pub mod vm;

#[cfg(test)]
mod in_pipeline_tests {
    use crate::compiler::{driver, icore, tree_front};
    use crate::core_ir::Decl;
    use crate::core_ir::{Expr, Stmt};
    use crate::hybrid_sil;
    use crate::in_lang_parse;
    use crate::lower_core;
    use crate::parser_registry::ParserId;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    struct TempDirGuard {
        path: PathBuf,
    }

    impl TempDirGuard {
        fn new(path: PathBuf) -> Self {
            Self { path }
        }
    }

    impl Drop for TempDirGuard {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[test]
    fn minimal_in_source_to_sil_contains_main() {
        let src = "fn main() -> void\n";
        let module = in_lang_parse::parse_in_source(src).expect("parse .in");
        let sil = lower_core::lower_to_textual_sil(&module, "App");
        assert!(
            sil.contains("sil @main"),
            "expected textual SIL to declare @main, got:\n{sil}"
        );
    }

    #[test]
    fn in_sample_shape_lowers_main_body() {
        let src = r#"
struct Session {
  Int id
  String label
}
fn note(text: String) -> void { return; }
fn main() -> void { let seed: Int = 0; note("ready"); return; }
"#;
        let module = in_lang_parse::parse_in_source(src).expect("parse .in");
        let sil = lower_core::lower_to_textual_sil(&module, "App");
        assert!(sil.contains("integer_literal $Builtin.Int64, 0"));
        assert!(sil.contains("function_ref @note"));
        assert!(sil.contains("return %"));
    }

    #[test]
    fn in_extern_call_lowers_graph_edge() {
        let src = r#"
import host.log;
capability process.stdout;
extern rust fn host_log(text: String) -> void;
fn main() -> void { host_log("ready"); return; }
"#;
        let module = in_lang_parse::parse_in_source(src).expect("parse .in");
        let sil = lower_core::lower_to_textual_sil(&module, "App");
        assert!(sil.contains("sil @host_log"), "sil:\n{sil}");
        assert!(sil.contains("sil @main"), "sil:\n{sil}");
        assert!(sil.contains("function_ref @host_log"), "sil:\n{sil}");
        let artifact = hybrid_sil::parse_textual_sil(&sil);
        let cleaned = hybrid_sil::remove_debug_insts(&artifact);
        let report = hybrid_sil::extract_call_graph(&cleaned);
        assert!(
            report
                .call_edges
                .contains(&("main".into(), "host_log".into())),
            "sil:\n{sil}"
        );
    }

    #[test]
    fn minimal_icore_json_to_sil_contains_main() {
        let j = r#"{
            "icoreVersion": 1,
            "decls": [
                { "kind": "struct", "name": "S", "fields": [{ "name": "x", "type": "Int" }] },
                { "kind": "function", "name": "main", "params": [], "return": "Void", "body": [] }
            ]
        }"#;
        let module = icore::parse_icore_source(j).expect("icore");
        let sil = driver::lower_unified_module(&module, "App");
        assert!(sil.contains("sil @main"), "sil:\n{sil}");
    }

    #[test]
    fn java_tree_front_lowers_to_textual_sil() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "inauguration-java-tree-front-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let _guard = TempDirGuard::new(temp_dir.clone());

        let path = temp_dir.join("Hello.java");
        fs::write(
            &path,
            r#"
public class Hello {
  private static int helper(int value) { return value; }
  private static int twice(int value) { return helper(value); }
  public static void main(String[] args) { twice(1); }
}
"#,
        )
        .expect("write Java source");

        let module = tree_front::parse_polyglot_file(ParserId::Java, &path).expect("parse Java");
        let sil = driver::lower_unified_module(&module, "App");
        assert!(sil.contains("sil @helper"), "sil:\n{sil}");
        assert!(sil.contains("sil @twice"), "sil:\n{sil}");
        assert!(sil.contains("sil @main"), "sil:\n{sil}");
        assert!(sil.contains("function_ref @helper"), "sil:\n{sil}");
        assert!(sil.contains("function_ref @twice"), "sil:\n{sil}");
        let artifact = hybrid_sil::parse_textual_sil(&sil);
        let cleaned = hybrid_sil::remove_debug_insts(&artifact);
        let report = hybrid_sil::extract_call_graph(&cleaned);
        assert!(
            report
                .call_edges
                .contains(&("twice".into(), "helper".into())),
            "sil:\n{sil}"
        );
        assert!(
            report.call_edges.contains(&("main".into(), "twice".into())),
            "sil:\n{sil}"
        );
        assert!(
            !artifact.instructions.is_empty() || !report.call_edges.is_empty(),
            "hybrid_sil should see instructions or call edges from lowered Java SIL; sil:\n{sil}"
        );
    }

    #[test]
    fn c_tree_front_lowers_trivial_return_and_helpers() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "inauguration-c-tree-front-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let _guard = TempDirGuard::new(temp_dir.clone());

        let path = temp_dir.join("sample.c");
        fs::write(
            &path,
            r#"int helper(void) { return 42; }
int main(void) { return 0; }
"#,
        )
        .expect("write C source");

        let module = tree_front::parse_polyglot_file(ParserId::C, &path).expect("parse C");
        let sil = driver::lower_unified_module(&module, "App");
        assert!(sil.contains("sil @helper"), "sil:\n{sil}");
        assert!(sil.contains("sil @main"), "sil:\n{sil}");
        assert!(
            sil.contains("integer_literal $Builtin.Int64, 42"),
            "helper return literal; sil:\n{sil}"
        );
        assert!(
            sil.contains("integer_literal $Builtin.Int64, 0"),
            "main return literal; sil:\n{sil}"
        );
        let artifact = hybrid_sil::parse_textual_sil(&sil);
        let cleaned = hybrid_sil::remove_debug_insts(&artifact);
        let report = hybrid_sil::extract_call_graph(&cleaned);
        assert!(
            !report
                .call_edges
                .contains(&("main".into(), "helper".into())),
            "C helper declarations should not synthesize call edges; sil:\n{sil}"
        );
        assert!(
            !artifact.instructions.is_empty() || !report.call_edges.is_empty(),
            "hybrid_sil should see instructions or call edges from lowered C SIL; sil:\n{sil}"
        );
    }

    #[test]
    fn c_tree_front_return_statement_uses_parameter_ident() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system clock before UNIX_EPOCH")
            .as_nanos();
        let temp_dir = std::env::temp_dir().join(format!(
            "inauguration-c-tree-front-ident-{}-{}",
            std::process::id(),
            unique
        ));
        fs::create_dir_all(&temp_dir).expect("create temp dir");
        let _guard = TempDirGuard::new(temp_dir.clone());

        let path = temp_dir.join("echo.c");
        fs::write(
            &path,
            "int echo(int x) { return x; }\nint main(void) { return 0; }\n",
        )
        .expect("write C source");

        let module = tree_front::parse_polyglot_file(ParserId::C, &path).expect("parse C");
        let echo = module
            .decls
            .iter()
            .find(|d| matches!(d, Decl::Function { name, .. } if name == "echo"))
            .expect("echo fn");
        match echo {
            Decl::Function { body, .. } => {
                assert_eq!(
                    body.as_slice(),
                    &[Stmt::Return(Some(Expr::Ident("x".into())))]
                );
            }
            _ => panic!("expected function"),
        }
        let sil = driver::lower_unified_module(&module, "App");
        assert!(sil.contains("sil @echo"), "sil:\n{sil}");
    }

    #[test]
    fn bytecode_backend_from_sil() {
        // Test the full pipeline: .in → Core IR → SIL → Bytecode → VM
        let src = "fn main() -> void { return; }";
        let module = in_lang_parse::parse_in_source(src).expect("parse .in");
        let sil = lower_core::lower_to_textual_sil(&module, "App");

        // Parse SIL into artifact
        let artifact = hybrid_sil::parse_textual_sil(&sil);
        assert!(
            artifact.function_id.contains("main"),
            "should identify main function"
        );

        // Lower to bytecode
        let bytecode_module = crate::sil_to_bytecode::lower_sil_to_bytecode(&artifact)
            .expect("lower SIL to bytecode");
        assert_eq!(bytecode_module.functions.len(), 1);
        assert_eq!(bytecode_module.functions[0].name, "main");

        // Execute bytecode
        let mut vm = crate::vm::BytecodeVM::new(bytecode_module);
        let result = vm.run().expect("bytecode execution");
        assert!(matches!(result, crate::bytecode::Value::Int(0)));
    }

    #[test]
    fn bytecode_with_helper_function() {
        // Test multi-function compilation
        let src = r#"
fn helper(x: Int) -> Int {
  return x
}
fn main() -> void {
  return
}
"#;
        let module = in_lang_parse::parse_in_source(src).expect("parse .in");
        let sil = lower_core::lower_to_textual_sil(&module, "App");

        // Parse SIL into artifact
        let artifact = hybrid_sil::parse_textual_sil(&sil);

        // Lower to bytecode
        let bytecode_module = crate::sil_to_bytecode::lower_sil_to_bytecode(&artifact)
            .expect("lower SIL to bytecode");

        // Should have at least 1 function (main)
        assert!(
            bytecode_module.functions.len() >= 1,
            "expected at least main function, got {} functions",
            bytecode_module.functions.len()
        );

        // Find main function
        let main_func = bytecode_module
            .functions
            .iter()
            .find(|f| f.name == "main")
            .expect("main function");
        assert!(
            !main_func.instructions.is_empty(),
            "main should have instructions"
        );

        // Execute
        let mut vm = crate::vm::BytecodeVM::new(bytecode_module);
        let result = vm.run().expect("bytecode execution");
        // Just verify it executes without error
        let _ = result;
    }

    #[test]
    fn bytecode_arithmetic_expression() {
        // Test that arithmetic SIL lowers to bytecode operations
        let src = "fn main() -> Int { let x: Int = 2 + 3; return x; }";
        let module = in_lang_parse::parse_in_source(src).expect("parse .in");
        let sil = lower_core::lower_to_textual_sil(&module, "App");
        assert!(
            sil.contains("integer_literal $Builtin.Int64"),
            "SIL should contain integer literals"
        );

        let artifact = hybrid_sil::parse_textual_sil(&sil);
        let bytecode_module = crate::sil_to_bytecode::lower_sil_to_bytecode(&artifact)
            .expect("lower SIL to bytecode");

        let mut vm = crate::vm::BytecodeVM::new(bytecode_module);
        let result = vm.run().expect("bytecode execution");
        // Should execute successfully
        let _ = result;
    }

    #[test]
    fn bytecode_unary_expression_from_in_source() {
        let src =
            "fn main() -> Int { let x: Int = 1; if !(x == 1) { return 0; } return -(x + 4); }";
        let module = in_lang_parse::parse_in_source(src).expect("parse .in");
        let sil = lower_core::lower_to_textual_sil(&module, "App");
        assert!(sil.contains("builtin_unop"));

        let artifact = hybrid_sil::parse_textual_sil(&sil);
        let bytecode_module = crate::sil_to_bytecode::lower_sil_to_bytecode(&artifact)
            .expect("lower SIL to bytecode");

        let mut vm = crate::vm::BytecodeVM::new(bytecode_module);
        let result = vm.run().expect("bytecode execution");
        assert_eq!(result.to_int(), -5);
    }

    #[test]
    fn bytecode_logical_expression_from_in_source() {
        let src = "fn main() -> Int { let x: Int = 2; if false || true && x == 2 { return 7; } return 0; }";
        let module = in_lang_parse::parse_in_source(src).expect("parse .in");
        let sil = lower_core::lower_to_textual_sil(&module, "App");
        assert!(sil.contains("builtin_binop \"||\""));
        assert!(sil.contains("builtin_binop \"&&\""));

        let artifact = hybrid_sil::parse_textual_sil(&sil);
        let bytecode_module = crate::sil_to_bytecode::lower_sil_to_bytecode(&artifact)
            .expect("lower SIL to bytecode");

        let mut vm = crate::vm::BytecodeVM::new(bytecode_module);
        let result = vm.run().expect("bytecode execution");
        assert_eq!(result.to_int(), 7);
    }
}
