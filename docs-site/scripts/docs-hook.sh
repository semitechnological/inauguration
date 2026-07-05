#!/usr/bin/env bash
# crepus [targets.docs] hook — cwd is docs-site/
set -euo pipefail
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
MANIFEST="$ROOT/docs-site/docs-gen/Cargo.toml"
cargo build --manifest-path "$MANIFEST" -q 2>/dev/null || cargo build --manifest-path "$MANIFEST" -q
exec cargo run --manifest-path "$MANIFEST" -q -- "$@"