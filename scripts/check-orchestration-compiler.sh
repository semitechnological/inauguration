#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

in_cmd=("${IN_BIN:-in}")
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
    ("distributed-workers", True, "local-distributed-simulator") in statuses,
    "graph orchestration missing local distributed simulator fact",
)
require(
    ("gpu-optimizer", False, "gpu-runtime-not-implemented") in statuses,
    "graph orchestration missing gpu status-only runtime fact",
)
plan = {
    (step.get("kind"), step.get("name"), step.get("mode"))
    for step in orch.get("local_plan") or []
}
require(
    ("parallel_task", "warm_cache", "local-deterministic-sequential") in plan,
    "graph orchestration missing local parallel task plan",
)
require(
    ("distributed_fn", "process_video", "local-worker-simulator") in plan,
    "graph orchestration missing local distributed function plan",
)
jobs = {
    (job.get("function"), job.get("worker"), job.get("status"))
    for job in orch.get("distributed_jobs") or []
}
require(
    ("process_video", "local-simulated-worker", "planned") in jobs,
    "graph orchestration missing local distributed job fact",
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
    ("distributed-workers", True, "local-distributed-simulator") in statuses,
    "agent orchestration missing local distributed simulator fact",
)
require(
    any(job.get("function") == "process_video" for job in orch.get("distributed_jobs") or []),
    "agent orchestration missing local distributed job fact",
)
PY

echo "check orchestration build local-plan core path"
"${in_cmd[@]}" build --path apps/in-sample/orchestration.in > "$tmp_dir/orchestration-build.txt"

echo "check bytecode execution examples"
"${in_cmd[@]}" execute-bytecode apps/in-sample/hello.in > "$tmp_dir/hello-bytecode.txt"
"${in_cmd[@]}" execute-bytecode apps/in-sample/agent-native.in > "$tmp_dir/agent-native-bytecode.txt"
"${in_cmd[@]}" execute-bytecode apps/in-sample/orchestration.in > "$tmp_dir/orchestration-bytecode.txt"

echo "check owned backend report"
backend_json="$tmp_dir/backend.json"
"${in_cmd[@]}" backend --path apps/in-sample/agent-native.in --target bytecode --json > "$backend_json"
python3 - "$backend_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

selected = data.get("selected") or {}
require(data.get("schema_version") == 1, "backend schema version was not 1")
require(selected.get("name") == "bytecode", "backend selected name was not bytecode")
require(selected.get("implemented") is True, "bytecode backend was not implemented")
require(selected.get("reason_code") == "bytecode-vm-subset", "backend reason code was not bytecode-vm-subset")
require(selected.get("artifact_kind") == "bytecode-assembly", "backend artifact kind was not bytecode-assembly")
request = data.get("request") or {}
require(request.get("supported") is True, "bytecode backend request was not supported")
artifact = data.get("artifact") or {}
require(artifact.get("entry_point") == "main", "backend artifact entry point was not main")
require((artifact.get("function_count") or 0) >= 1, "backend artifact function count was empty")
available = {
    (backend.get("name"), backend.get("implemented"), backend.get("reason_code"))
    for backend in data.get("available") or []
}
native_contract = ("native", False, "native-backend-not-implemented")
native_aarch64 = ("native", True, "native-aarch64-subset")
require(
    native_contract in available or native_aarch64 in available,
    "native backend status was not explicit",
)
PY

native_backend_json="$tmp_dir/native-backend.json"
"${in_cmd[@]}" backend --path apps/in-sample/agent-native.in --target native --json > "$native_backend_json"
if [[ "$(uname -s)" == "Darwin" && "$(uname -m)" == "arm64" ]]; then
  NATIVE_BACKEND_MODE="aarch64"
else
  NATIVE_BACKEND_MODE="contract"
fi
python3 - "$native_backend_json" "$NATIVE_BACKEND_MODE" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())
mode = sys.argv[2]

def require(condition, message):
    if not condition:
        raise SystemExit(message)

selected = data.get("selected") or {}
require(data.get("schema_version") == 1, "native backend schema version was not 1")
require(selected.get("name") == "native", "native backend selected name was not native")
request = data.get("request") or {}
require(data.get("artifact") is None, "native backend unexpectedly reported an artifact")
if mode == "aarch64":
    require(selected.get("implemented") is True, "native backend should be implemented on macOS aarch64")
    require(
        selected.get("reason_code") == "native-aarch64-subset",
        "native backend reason code mismatch",
    )
    require(request.get("supported") is False, "native backend request should not claim artifact without compile path")
else:
    require(selected.get("implemented") is False, "native backend must remain status-only")
    require(
        selected.get("reason_code") == "native-backend-not-implemented",
        "native backend reason code mismatch",
    )
    require(request.get("supported") is False, "native backend request unexpectedly reported supported")
    require(
        request.get("reason_code") == "native-backend-not-implemented",
        "native backend request reason code mismatch",
    )
PY

echo "check package json"
package_json="$tmp_dir/package.json"
"${in_cmd[@]}" package --path apps/package-sample/main.in --json > "$package_json"
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

selection = data.get("target_selection") or {}
require(set(selection.get("enabled") or []) == {"linux", "macos", "web"}, "package enabled target selection mismatch")

policy = data.get("capability_policy") or {}
require(policy.get("valid") is True, "package empty capability policy should be valid")

graph = data.get("package_graph") or {}
nodes = {
    (node.get("kind"), node.get("id"))
    for node in graph.get("nodes") or []
}
require(("package", "package:hyperchat") in nodes, "package graph missing package node")
require(("extension", "extension:distributed-workers") in nodes, "package graph missing distributed-workers extension node")

identity = data.get("source_identity") or {}
require(identity.get("status") == "match", "package source identity was not match")

semantic_imports = data.get("semantic_imports") or []
require(
    any(item.get("import") == "database.postgres" and item.get("status") == "resolved" for item in semantic_imports),
    "package semantic import did not resolve database.postgres",
)

symbol_index = data.get("symbol_index") or []
require(
    any(item.get("id") == "symbol:dependency:postgres" for item in symbol_index),
    "package symbol index missing postgres dependency",
)
require(data.get("diagnostics") == [], "package report unexpectedly had diagnostics")
PY

echo "check package graph semantic import symbols"
package_graph_json="$tmp_dir/package-graph.json"
"${in_cmd[@]}" graph --path apps/package-sample/main.in --json > "$package_graph_json"
python3 - "$package_graph_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

symbols = {
    (symbol.get("kind"), symbol.get("name"), symbol.get("detail"))
    for symbol in data.get("symbols") or []
}
require(
    ("dependency", "postgres", "database.postgres") in symbols,
    "package graph missing postgres dependency symbol",
)
require(data.get("package_diagnostics") == [], "package graph unexpectedly had diagnostics")
PY

echo "check unresolved package semantic import"
missing_package_json="$tmp_dir/package-missing-import.json"
"${in_cmd[@]}" package --path apps/package-sample/missing-import.in --json > "$missing_package_json"
python3 - "$missing_package_json" <<'PY'
import json
import sys
from pathlib import Path

data = json.loads(Path(sys.argv[1]).read_text())

def require(condition, message):
    if not condition:
        raise SystemExit(message)

diagnostics = data.get("diagnostics") or []
require(
    any(item.get("code") == "INPKG001" and item.get("severity") == "warning" for item in diagnostics),
    "missing package semantic import did not produce INPKG001 warning",
)
require(data.get("symbol_index") == [], "unresolved package import should not create symbols")
PY

echo "orchestration compiler checks passed"
