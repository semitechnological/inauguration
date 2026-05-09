//! Library crate backing the `in` CLI — hybrid compiler wave plus embedded hotreload daemon.

#[cfg(unix)]
pub mod preview_client;

pub mod compiler;
pub mod core_ir;
pub mod hotreload;
pub mod hybrid_core;
pub mod hybrid_pipeline;
pub mod hybrid_scheduler;
pub mod hybrid_sil;
pub mod in_lang_parse;
pub mod lower_core;
pub mod native_swift_sil;
pub mod parser_registry;
pub mod sil_emit;
pub mod swift_subset;

#[cfg(test)]
mod in_pipeline_tests {
    use crate::compiler::{driver, icore, tree_front};
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
fn main() -> void { let seed: Int = 0; return; }
"#;
        let module = in_lang_parse::parse_in_source(src).expect("parse .in");
        let sil = lower_core::lower_to_textual_sil(&module, "App");
        assert!(sil.contains("integer_literal $Builtin.Int64, 0"));
        assert!(sil.contains("function_ref @note"));
        assert!(sil.contains("return %"));
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
            "public class Hello { public static void main(String[] args) { } }",
        )
        .expect("write Java source");

        let module = tree_front::parse_polyglot_file(ParserId::Java, &path).expect("parse Java");
        let sil = driver::lower_unified_module(&module, "App");
        assert!(sil.contains("sil @main"), "sil:\n{sil}");
        let artifact = hybrid_sil::parse_textual_sil(&sil);
        let cleaned = hybrid_sil::remove_debug_insts(&artifact);
        let report = hybrid_sil::extract_call_graph(&cleaned);
        assert!(
            !artifact.instructions.is_empty() || !report.call_edges.is_empty(),
            "hybrid_sil should see instructions or call edges from lowered Java SIL; sil:\n{sil}"
        );
    }
}
