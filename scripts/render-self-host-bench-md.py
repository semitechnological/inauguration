#!/usr/bin/env python3
"""Inject self-host-vs-native.json into docs/benchmarks/self-host-vs-native.md."""
import json
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
MD = ROOT / "docs/benchmarks/self-host-vs-native.md"
JSON = ROOT / "docs/benchmarks/self-host-vs-native.json"
START = "<!-- BENCH:SELF_HOST_START -->"
END = "<!-- BENCH:SELF_HOST_END -->"


def main() -> int:
    if not JSON.is_file():
        print(f"missing {JSON}", file=sys.stderr)
        return 1
    data = json.loads(JSON.read_text())
    sh = data["self_host_parse"]
    nat = data["native_self_build"]
    block = f"""| Field | Value |
|-------|------:|
| Generated (UTC) | {data.get("generated_at_utc", "?")} |
| `in` version | {data.get("in_version", "?")} |
| Host / CPU | {data.get("host_os", "?")} / {data.get("cpu", "?")} |
| Functions parsed | {sh.get("functions_parsed", 0):,} |
| Functions typed | {sh.get("functions_typed", 0):,} |
| Call edges | {sh.get("call_edges", 0):,} |
| Wall ms (avg / runs) | {sh.get("wall_ms_avg", 0):.1f} / {sh.get("wall_ms_runs", [])} |
| JIT compile µs | {sh.get("jit_compile_us", 0):,} |
| Front+JIT OK | {sh.get("front_ok", False)} |
| Native `--out` | {nat.get("status", "?")} — {nat.get("note", "")[:120]} |"""
    text = MD.read_text()
    if START not in text or END not in text:
        print("markers missing in md", file=sys.stderr)
        return 1
    pre, rest = text.split(START, 1)
    _, post = rest.split(END, 1)
    MD.write_text(f"{pre}{START}\n{block}\n{END}{post}")
    print(MD)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())