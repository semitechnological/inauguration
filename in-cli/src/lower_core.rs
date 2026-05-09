//! Lower [`crate::core_ir::UnifiedModule`] to textual SIL matching `native_swift_sil` stubs.

use crate::core_ir::{Decl, UnifiedModule};

/// Emit textual SIL: helper functions first (sorted), then `@main` with `function_ref` callees and a
/// unique SSA id space (same contract as [`crate::native_swift_sil`]).
pub fn lower_to_textual_sil(module: &UnifiedModule, _module_id: &str) -> String {
    let mut fn_names: Vec<String> = module
        .decls
        .iter()
        .filter_map(|d| match d {
            Decl::Function { name, .. } => Some(name.clone()),
            _ => None,
        })
        .collect();
    fn_names.sort();
    let mut sil = String::from("// inauguration core → textual SIL (multi-front v0)\n");
    let helpers: Vec<&str> = fn_names
        .iter()
        .map(String::as_str)
        .filter(|n| *n != "main")
        .collect();

    let mut ssa = 0usize;
    for name in &fn_names {
        if *name == "main" {
            continue;
        }
        let v = ssa;
        ssa += 1;
        sil.push_str(&format!(
            "sil @{name}\nbb0:\n%{v} = integer_literal $Builtin.Int64, 0\nbb1:\nreturn %{v} : $Builtin.Int64\n"
        ));
    }

    sil.push_str("sil @main\nbb0:\n");
    for callee in &helpers {
        let r = ssa;
        ssa += 1;
        sil.push_str(&format!(
            "%{r} = function_ref @{callee} : $@convention(thin)\n"
        ));
    }
    let ret = ssa;
    sil.push_str(&format!("%{ret} = integer_literal $Builtin.Int64, 0\n"));
    sil
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core_ir::Typ;

    #[test]
    fn lower_orders_helpers_and_main() {
        let module = UnifiedModule {
            decls: vec![
                Decl::Struct {
                    name: "S".into(),
                    fields: vec![],
                },
                Decl::Function {
                    name: "zeta".into(),
                    params: vec![],
                    ret: Typ::Void,
                    body: vec![],
                },
                Decl::Function {
                    name: "main".into(),
                    params: vec![],
                    ret: Typ::Void,
                    body: vec![],
                },
                Decl::Function {
                    name: "alpha".into(),
                    params: vec![],
                    ret: Typ::Void,
                    body: vec![],
                },
            ],
        };
        let sil = lower_to_textual_sil(&module, "App");
        assert!(sil.contains("sil @main"));
        assert!(sil.contains("sil @alpha"));
        assert!(sil.contains("sil @zeta"));
        let pa = sil.find("function_ref @alpha").expect("alpha");
        let pz = sil.find("function_ref @zeta").expect("zeta");
        assert!(pa < pz);
    }
}
