//! Library crate backing the `in` CLI — hybrid compiler wave plus embedded hotreload daemon.

#[cfg(unix)]
pub mod preview_client;

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
    use crate::in_lang_parse;
    use crate::lower_core;

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
}
