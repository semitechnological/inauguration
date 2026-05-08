#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SOCKET="$ROOT/.brisk/hotreload/daemon.sock"
METRICS="$ROOT/.brisk/hotreload/metrics/latest.ndjson"
WATCH_ROOT="$ROOT/apps/sample-swiftui"

cleanup() {
  if [[ -n "${DAEMON_PID:-}" ]] && kill -0 "$DAEMON_PID" 2>/dev/null; then
    kill "$DAEMON_PID" || true
  fi
}
trap cleanup EXIT INT TERM

mkdir -p "$(dirname "$SOCKET")" "$(dirname "$METRICS")"

(
  cd "$ROOT/runtime/hotreload-daemon"
  cargo run -- "$WATCH_ROOT" "$SOCKET" "$METRICS" 60
) &
DAEMON_PID=$!

sleep 1

cd "$ROOT/runtime/swift-preview-host"
swift run swift-preview-host-client "$SOCKET"
