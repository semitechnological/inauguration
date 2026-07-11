use crate::core_ir::{Decl, Expr, LoopKind, Stmt, Typ, UnifiedModule};
use crate::in_lang_parse::parse_in_source;

pub fn canonicalize_in_source(source: &str) -> Result<String, String> {
    let module = parse_in_source(source)?;
    Ok(format_module(&module))
}

fn format_module(module: &UnifiedModule) -> String {
    let mut out = String::new();
    for (idx, decl) in module.decls.iter().enumerate() {
        if idx > 0 {
            out.push('\n');
            out.push('\n');
        }
        format_decl(decl, &mut out);
    }
    out.push('\n');
    out
}

fn format_decl(decl: &Decl, out: &mut String) {
    match decl {
        Decl::Struct { name, fields, .. } => {
            out.push_str("struct ");
            out.push_str(name);
            out.push_str(" {\n");
            for (field, typ) in fields {
                out.push_str("  ");
                out.push_str(&format_type(typ));
                out.push(' ');
                out.push_str(field);
                out.push('\n');
            }
            out.push('}');
        }
        Decl::Function {
            name,
            params,
            ret,
            body,
            ..
        } => {
            out.push_str("fn ");
            out.push_str(name);
            out.push('(');
            for (idx, (param, typ)) in params.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(param);
                out.push_str(": ");
                out.push_str(&format_type(typ));
            }
            out.push_str(") -> ");
            out.push_str(&format_type(ret));
            out.push_str(" {\n");
            format_body(body, 1, out);
            out.push('}');
        }
        Decl::Class { .. } | Decl::Interface { .. } | Decl::Component { .. } => {}
        Decl::Global { .. } => {}
    }
}

fn format_type(typ: &Typ) -> String {
    match typ {
        Typ::Int => "Int".into(),
        Typ::Float => "Float".into(),
        Typ::String => "String".into(),
        Typ::Bool => "Bool".into(),
        Typ::Void => "void".into(),
        Typ::Array(item) => format!("[{}]", format_type(item)),
        Typ::Vector(item) => format!("Vec<{}>", format_type(item)),
        Typ::Named(name) => name.clone(),
        Typ::Generic(name) => name.clone(),
    }
}

fn format_body(body: &[Stmt], depth: usize, out: &mut String) {
    for stmt in body {
        format_stmt(stmt, depth, out);
    }
}

fn format_stmt(stmt: &Stmt, depth: usize, out: &mut String) {
    let indent = "  ".repeat(depth);
    match stmt {
        Stmt::Let(name, typ, expr) => {
            out.push_str(&indent);
            out.push_str("let ");
            out.push_str(name);
            if let Some(typ) = typ {
                out.push_str(": ");
                out.push_str(&format_type(typ));
            }
            out.push_str(" = ");
            out.push_str(&format_expr(expr));
            out.push('\n');
        }
        Stmt::Assign(name, expr) => {
            out.push_str(&indent);
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(&format_expr(expr));
            out.push('\n');
        }
        Stmt::FieldAssign {
            base, name, value, ..
        } => {
            out.push_str(&indent);
            out.push_str(&format_expr(base));
            out.push('.');
            out.push_str(name);
            out.push_str(" = ");
            out.push_str(&format_expr(value));
            out.push('\n');
        }
        Stmt::IndexAssign {
            base, index, value, ..
        } => {
            out.push_str(&indent);
            out.push_str(&format_expr(base));
            out.push('[');
            out.push_str(&format_expr(index));
            out.push_str("] = ");
            out.push_str(&format_expr(value));
            out.push('\n');
        }
        Stmt::Return(None) => {
            out.push_str(&indent);
            out.push_str("return\n");
        }
        Stmt::Break => {}
        Stmt::Return(Some(expr)) => {
            out.push_str(&indent);
            out.push_str("return ");
            out.push_str(&format_expr(expr));
            out.push('\n');
        }
        Stmt::Expr(expr) => {
            out.push_str(&indent);
            out.push_str(&format_expr(expr));
            out.push('\n');
        }
        Stmt::If {
            cond,
            then_body,
            else_body,
        } => {
            out.push_str(&indent);
            out.push_str("if ");
            out.push_str(&format_expr(cond));
            out.push_str(" {\n");
            format_body(then_body, depth + 1, out);
            out.push_str(&indent);
            out.push('}');
            if else_body.is_empty() {
                out.push('\n');
            } else {
                out.push_str(" else {\n");
                format_body(else_body, depth + 1, out);
                out.push_str(&indent);
                out.push_str("}\n");
            }
        }
        Stmt::Loop {
            kind: LoopKind::While,
            cond: Some(cond),
            body,
        } => {
            out.push_str(&indent);
            out.push_str("while ");
            out.push_str(&format_expr(cond));
            out.push_str(" {\n");
            format_body(body, depth + 1, out);
            out.push_str(&indent);
            out.push_str("}\n");
        }
        Stmt::Loop { body, .. } => {
            out.push_str(&indent);
            out.push_str("while true {\n");
            format_body(body, depth + 1, out);
            out.push_str(&indent);
            out.push_str("}\n");
        }
        Stmt::Match {
            scrutinee, arms, ..
        } => {
            out.push_str(&indent);
            out.push_str("match ");
            out.push_str(&format_expr(scrutinee));
            out.push_str(" {\n");
            for arm in arms {
                out.push_str(&indent);
                out.push_str("  ");
                out.push_str(&arm.pattern);
                out.push_str(" {\n");
                format_body(&arm.body, depth + 2, out);
                out.push_str(&indent);
                out.push_str("  }\n");
            }
            out.push_str(&indent);
            out.push_str("}\n");
        }
        Stmt::Throw(_) | Stmt::Try { .. } | Stmt::Propagate => {}
    }
}

fn format_expr(expr: &Expr) -> String {
    match expr {
        Expr::IntLit(n) => n.to_string(),
        Expr::FloatLit(f) => f.0.to_string(),
        Expr::StringLit(value) => format!("{value:?}"),
        Expr::BoolLit(value) => value.to_string(),
        Expr::Ident(name) => name.clone(),
        Expr::Unary { op, expr, .. } => {
            let mut out = op.clone();
            out.push_str(&format_expr(expr));
            out
        }
        Expr::Binary { op, lhs, rhs, .. } => {
            format!("{} {} {}", format_expr(lhs), op, format_expr(rhs))
        }
        Expr::StructInit { name, fields, .. } => {
            let rendered = fields
                .iter()
                .map(|(field, expr)| format!("{field}: {}", format_expr(expr)))
                .collect::<Vec<_>>()
                .join(", ");
            format!("{name} {{ {rendered} }}")
        }
        Expr::Field { base, name, .. } => {
            format!("{}.{}", format_expr(base), name)
        }
        Expr::ArrayLit(items) => {
            let rendered = items.iter().map(format_expr).collect::<Vec<_>>().join(", ");
            format!("[{rendered}]")
        }
        Expr::Index { base, index, .. } => {
            format!("{}[{}]", format_expr(base), format_expr(index))
        }
        Expr::Call { callee, args, .. } => {
            if let Expr::Ident(name) = callee.as_ref()
                && let Some(method) = name.strip_prefix("__method__")
                && let Some((base, rest)) = args.split_first()
            {
                let rendered_args = rest.iter().map(format_expr).collect::<Vec<_>>().join(", ");
                return format!("{}.{}({rendered_args})", format_expr(base), method);
            }
            let mut out = format_expr(callee);
            out.push('(');
            for (idx, arg) in args.iter().enumerate() {
                if idx > 0 {
                    out.push_str(", ");
                }
                out.push_str(&format_expr(arg));
            }
            out.push(')');
            out
        }
        Expr::Closure { .. } => "closure".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::in_lang_parse::parse_in_source;

    #[test]
    fn canonicalizes_example_style_source() {
        let src = r#"
fn main() { printone(); }
fn printone() {
    print("1");
}
"#;

        let canonical = canonicalize_in_source(src).expect("canonicalize");

        assert_eq!(
            canonical,
            r#"fn main() -> void {
  printone()
}

fn printone() -> void {
  print("1")
}
"#
        );
    }

    #[test]
    fn canonical_output_remains_parseable() {
        let src = r#"
struct Session {
  Int id
  String label
}
fn note(text: String) -> VOID { return; }
fn main() {
  let seed: Int = 0;
  note("ready");
  return;
}
"#;

        let canonical = canonicalize_in_source(src).expect("canonicalize");

        parse_in_source(&canonical).expect("canonical output parses");
        assert!(canonical.contains("  Int id\n"));
        assert!(canonical.contains("  String label\n"));
        assert!(canonical.contains("fn note(text: String) -> void {\n"));
        assert!(canonical.contains("  let seed: Int = 0\n"));
        assert!(!canonical.contains(';'));
    }
}
