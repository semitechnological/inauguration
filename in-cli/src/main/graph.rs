use crate::{InError, Result};
use inauguration::graph_report;
use inauguration::parser_registry::ParserCli;
use std::path::Path;

pub(crate) fn cmd_graph(
    invocation_cwd: &Path,
    path: &str,
    module_id: &str,
    parser: ParserCli,
    selection: graph_report::GraphReportSelection,
    json: bool,
) -> Result<()> {
    let report =
        graph_report::build_graph_report(invocation_cwd, path, module_id, parser, selection);
    if json {
        let raw = graph_report::graph_report_to_json(&report)
            .map_err(|err| InError::Message(format!("serialize graph report: {err}")))?;
        println!("{raw}");
    } else {
        println!("{}", graph_report::graph_report_text(&report, selection));
    }
    Ok(())
}
