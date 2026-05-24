#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

in_cmd=(cargo run --quiet --manifest-path in-cli/Cargo.toml --bin in --)
tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

echo "check canonicalize example.in"
"${in_cmd[@]}" canonicalize --path example.in --check

echo "check graph json"
graph_json="$tmp_dir/graph.json"
"${in_cmd[@]}" graph --path apps/in-sample/agent-native.in --json > "$graph_json"
python3 - "$graph_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

parser = data.get("parser_decision") or {}
require(parser.get("parser_id") == "in", "graph parser_id was not in")
require(parser.get("route") == "core_ir", "graph route was not core_ir")
require(data.get("entry_function") == "main", "graph entry function was not main")

calls = {
    (edge.get("caller"), edge.get("callee"))
    for edge in data.get("call_edges") or []
}
require(("main", "print") in calls, "graph missing main -> print call edge")
require(("main", "ready") in calls, "graph missing main -> ready call edge")

capabilities = set(data.get("capabilities") or [])
effects = set(data.get("effects") or [])
require(
    "process.stdout" in capabilities
    or "extern:std:print:requires=process.stdout" in effects,
    "graph missing process.stdout capability facts",
)

symbols = {
    (symbol.get("kind"), symbol.get("name"))
    for symbol in data.get("symbols") or []
}
require(("function", "main") in symbols, "graph missing main symbol")
require(("function", "print") in symbols, "graph missing print symbol")

timing = data.get("timing") or {}
require(isinstance(timing.get("total_micros"), int), "graph missing total_micros timing")
PY

echo "check orchestration graph json"
orchestration_graph_json="$tmp_dir/orchestration-graph.json"
"${in_cmd[@]}" graph --path apps/in-sample/orchestration.in --json > "$orchestration_graph_json"
python3 - "$orchestration_graph_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

orch = data.get("orchestration") or {}
require(
    "distributed-workers" in (orch.get("enabled_extensions") or []),
    "graph orchestration missing distributed-workers extension",
)
require(
    "gpu-optimizer" in (orch.get("enabled_extensions") or []),
    "graph orchestration missing gpu-optimizer extension",
)
require(
    "process_video" in (orch.get("distributed_functions") or []),
    "graph orchestration missing distributed process_video fact",
)
require(orch.get("parallel_regions") == 1, "graph orchestration parallel region count was not 1")

statuses = {
    (status.get("name"), status.get("implemented"), status.get("reason_code"))
    for status in orch.get("runtime_status") or []
}
require(
    ("distributed-workers", False, "distributed-runtime-not-implemented") in statuses,
    "graph orchestration missing distributed status-only runtime fact",
)
require(
    ("gpu-optimizer", False, "gpu-runtime-not-implemented") in statuses,
    "graph orchestration missing gpu status-only runtime fact",
)
PY

echo "check orchestration agent json"
agent_json="$tmp_dir/orchestration-agent.json"
"${in_cmd[@]}" agent --path apps/in-sample/orchestration.in > "$agent_json"
python3 - "$agent_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

require(not data.get("diagnostics"), "agent report had diagnostics")
orch = data.get("orchestration") or {}
require(
    "process_video" in (orch.get("distributed_functions") or []),
    "agent orchestration missing distributed process_video fact",
)
require(orch.get("parallel_regions") == 1, "agent orchestration parallel region count was not 1")
statuses = {
    (status.get("name"), status.get("implemented"), status.get("reason_code"))
    for status in orch.get("runtime_status") or []
}
require(
    ("distributed-workers", False, "distributed-runtime-not-implemented") in statuses,
    "agent orchestration missing distributed status-only runtime fact",
)
PY

echo "check orchestration build status-only core path"
"${in_cmd[@]}" build --path apps/in-sample/orchestration.in > "$tmp_dir/orchestration-build.txt"

echo "check package json"
package_json="$tmp_dir/package.json"
"${in_cmd[@]}" package --path apps/package-sample --json > "$package_json"
python3 - "$package_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

require(data.get("name") == "hyperchat", "package name was not hyperchat")

targets = data.get("targets") or {}
require(targets.get("linux") is True, "package missing linux target")
require(targets.get("macos") is True, "package missing macos target")
require(targets.get("web") is True, "package missing web target")

capabilities = set(data.get("capabilities") or [])
require("network.http" in capabilities, "package missing network.http capability")
require("filesystem.read" in capabilities, "package missing filesystem.read capability")

extensions = set(data.get("extensions") or [])
require("distributed-workers" in extensions, "package missing distributed-workers extension")
require("gpu-optimizer" in extensions, "package missing gpu-optimizer extension")
PY

echo "orchestration compiler checks passed"
