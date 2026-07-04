use crate::util::resolve_invocation_path;
use crate::{InError, Result};
use inauguration::agent_mode;
use inauguration::parser_registry::{ParserCli, ParserId};
use std::fs;
use std::path::Path;

pub(crate) fn cmd_agent(
    invocation_cwd: &Path,
    path: &str,
    module_id: &str,
    parser: ParserCli,
) -> Result<()> {
    let report = agent_mode::analyze_path(invocation_cwd, path, module_id, parser);
    let json = serde_json::to_string_pretty(&report)
        .map_err(|err| InError::Message(format!("serialize agent report: {err}")))?;
    println!("{json}");
    if report
        .diagnostics
        .iter()
        .any(|diagnostic| matches!(diagnostic.severity, agent_mode::DiagnosticSeverity::Error))
    {
        Err(InError::Message("agent diagnostics failed".to_string()))
    } else {
        Ok(())
    }
}

pub(crate) fn cmd_explain(diagnostic_code: &str, json: bool) -> Result<()> {
    let Some(rule) = agent_mode::explain_diagnostic(diagnostic_code) else {
        return Err(InError::Message(format!(
            "unknown diagnostic code: {diagnostic_code}"
        )));
    };
    if json {
        let raw = serde_json::to_string_pretty(&rule)
            .map_err(|err| InError::Message(format!("serialize diagnostic rule: {err}")))?;
        println!("{raw}");
    } else {
        println!("{}", rule.code);
        println!("{}", rule.meaning);
        println!("fix: {}", rule.fix);
    }
    Ok(())
}

pub(crate) fn cmd_fix(
    invocation_cwd: &Path,
    plan: bool,
    json: bool,
    path: &str,
    module_id: &str,
    parser: ParserCli,
) -> Result<()> {
    if !plan {
        return Err(InError::Message(
            "`in fix` currently requires --plan so agents review typed edits before applying"
                .to_string(),
        ));
    }
    let report = agent_mode::fix_plan(invocation_cwd, path, module_id, parser);
    if json {
        let raw = serde_json::to_string_pretty(&report)
            .map_err(|err| InError::Message(format!("serialize fix plan: {err}")))?;
        println!("{raw}");
    } else {
        println!("repair plans: {}", report.repair_plans.len());
        for plan in &report.repair_plans {
            println!("{}: {}", plan.applies_to_code, plan.title);
            println!("  {}", plan.rationale);
            for action in &plan.actions {
                println!("  {}: {}", action.kind, action.description);
            }
        }
    }
    Ok(())
}

pub(crate) fn cmd_canonicalize(invocation_cwd: &Path, path: &str, check: bool) -> Result<()> {
    let source_path = resolve_invocation_path(invocation_cwd, path);
    let source = fs::read_to_string(&source_path)?;
    let canonical = inauguration::in_canonical::canonicalize_in_source(&source)
        .map_err(|err| InError::Message(format!("canonicalize: {err}")))?;
    if check {
        if source == canonical {
            return Ok(());
        }
        return Err(InError::Message(format!(
            "{} is not canonical",
            source_path.display()
        )));
    }
    print!("{canonical}");
    Ok(())
}

pub(crate) fn cmd_languages(json: bool) -> Result<()> {
    let entries = inauguration::language_support::all_language_support();
    if json {
        let reports: Vec<_> = entries
            .iter()
            .map(inauguration::boundary_capability::language_support_json)
            .collect();
        let raw = serde_json::to_string_pretty(&reports)
            .map_err(|err| InError::Message(format!("serialize language support: {err}")))?;
        println!("{raw}");
        return Ok(());
    }

    println!(
        "{:<12} {:<12} {:<18} {:<34} runtime",
        "language", "parser", "capabilities", "front"
    );
    for entry in entries {
        let caps_str = entry.capabilities.join(", ");
        println!(
            "{:<12} {:<12} {:<18} {:<34} {}",
            entry.language,
            entry.parser_id.unwrap_or("swift"),
            caps_str,
            entry.front,
            entry.runtime_boundary
        );
    }
    Ok(())
}

pub(crate) fn cmd_ocaml(invocation_cwd: &Path, path: &str) -> Result<()> {
    let resolved = if std::path::Path::new(path).is_absolute() {
        std::path::PathBuf::from(path)
    } else {
        invocation_cwd.join(path)
    };
    let module =
        inauguration::compiler::tree_front::parse_polyglot_file(ParserId::OCaml, &resolved)
            .map_err(|e| InError::Message(format!("ocaml front: {e}")))?;
    println!("parsed {} declarations", module.decls.len());
    for (i, decl) in module.decls.iter().enumerate() {
        println!("  {}: {:?}", i + 1, decl);
    }
    Ok(())
}
