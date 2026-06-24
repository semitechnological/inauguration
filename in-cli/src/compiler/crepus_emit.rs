//! Crepuscularity codegen — emits native UI source from CrepusIr.
//!
//! ponytail: minimal string-builder codegen. Replace with crepuscularity-native
//! crate integration when crate versioning aligns.

use super::crepus_ir::CrepusIr;

/// Target for native UI codegen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodegenTarget {
    SwiftUI,
    Compose,
    Html,
}

/// Emit native source for the given target.
pub fn emit_native(ir: &CrepusIr, target: CodegenTarget) -> Result<String, String> {
    match target {
        CodegenTarget::SwiftUI => emit_swiftui(ir),
        CodegenTarget::Compose => emit_compose(ir),
        CodegenTarget::Html => emit_html(ir),
    }
}

fn emit_swiftui(ir: &CrepusIr) -> Result<String, String> {
    let mut out = String::new();
    out.push_str(&format!("import SwiftUI\n\nstruct {}: View {{\n", ir.component));
    out.push_str("    var body: some View {\n");
    emit_view_swiftui(&ir.view_tree, &mut out, 2)?;
    out.push_str("    }\n}\n");
    Ok(out)
}

fn emit_view_swiftui(nodes: &[super::crepus_ir::ViewNode], out: &mut String, depth: usize) -> Result<(), String> {
    let indent = "    ".repeat(depth);
    for node in nodes {
        match node.kind.as_str() {
            "Window" | "VStack" => {
                out.push_str(&format!("{}VStack {{\n", indent));
                emit_view_swiftui(&node.children, out, depth + 1)?;
                out.push_str(&format!("{}}}\n", indent));
            }
            "HStack" => {
                out.push_str(&format!("{}HStack {{\n", indent));
                emit_view_swiftui(&node.children, out, depth + 1)?;
                out.push_str(&format!("{}}}\n", indent));
            }
            "Text" => {
                let content = node.attrs.iter()
                    .find(|(k, _)| k == "content")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                out.push_str(&format!("{}Text(\"{}\")\n", indent, content));
            }
            "Button" => {
                let label = node.attrs.iter()
                    .find(|(k, _)| k == "label")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("Button");
                out.push_str(&format!("{}Button(\"{}\") {{ }}\n", indent, label));
            }
            _ => {
                out.push_str(&format!("{}Text(\"<{}>\")\n", indent, node.kind));
            }
        }
    }
    Ok(())
}

fn emit_compose(ir: &CrepusIr) -> Result<String, String> {
    let mut out = String::new();
    out.push_str(&format!("@Composable\nfun {}() {{\n", ir.component));
    emit_view_compose(&ir.view_tree, &mut out, 1)?;
    out.push_str("}\n");
    Ok(out)
}

fn emit_view_compose(nodes: &[super::crepus_ir::ViewNode], out: &mut String, depth: usize) -> Result<(), String> {
    let indent = "    ".repeat(depth);
    for node in nodes {
        match node.kind.as_str() {
            "Window" | "VStack" | "Column" => {
                out.push_str(&format!("{}Column {{\n", indent));
                emit_view_compose(&node.children, out, depth + 1)?;
                out.push_str(&format!("{}}}\n", indent));
            }
            "HStack" | "Row" => {
                out.push_str(&format!("{}Row {{\n", indent));
                emit_view_compose(&node.children, out, depth + 1)?;
                out.push_str(&format!("{}}}\n", indent));
            }
            "Text" => {
                let content = node.attrs.iter()
                    .find(|(k, _)| k == "content")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                out.push_str(&format!("{}Text(text = \"{}\")\n", indent, content));
            }
            "Button" => {
                let label = node.attrs.iter()
                    .find(|(k, _)| k == "label")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("Button");
                out.push_str(&format!("{}Button(onClick = {{ }}) {{\n", indent));
                out.push_str(&format!("{}    Text(\"{}\")\n", indent, label));
                out.push_str(&format!("{}}}\n", indent));
            }
            _ => {}
        }
    }
    Ok(())
}

fn emit_html(ir: &CrepusIr) -> Result<String, String> {
    let mut out = String::new();
    out.push_str("<!DOCTYPE html>\n<html>\n<head>\n");
    out.push_str(&format!("  <title>{}</title>\n", ir.component));
    out.push_str("</head>\n<body>\n");
    emit_view_html(&ir.view_tree, &mut out, 1)?;
    out.push_str("</body>\n</html>\n");
    Ok(out)
}

fn emit_view_html(nodes: &[super::crepus_ir::ViewNode], out: &mut String, _depth: usize) -> Result<(), String> {
    for node in nodes {
        match node.kind.as_str() {
            "Window" | "VStack" | "Column" => {
                out.push_str("  <div class=\"vstack\">\n");
                emit_view_html(&node.children, out, _depth + 1)?;
                out.push_str("  </div>\n");
            }
            "HStack" | "Row" => {
                out.push_str("  <div class=\"hstack\">\n");
                emit_view_html(&node.children, out, _depth + 1)?;
                out.push_str("  </div>\n");
            }
            "Text" => {
                let content = node.attrs.iter()
                    .find(|(k, _)| k == "content")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("");
                out.push_str(&format!("  <p>{}</p>\n", content));
            }
            "Button" => {
                let label = node.attrs.iter()
                    .find(|(k, _)| k == "label")
                    .map(|(_, v)| v.as_str())
                    .unwrap_or("Button");
                out.push_str(&format!("  <button>{}</button>\n", label));
            }
            _ => {}
        }
    }
    Ok(())
}
