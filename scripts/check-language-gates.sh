#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo test --manifest-path in-cli/Cargo.toml --lib boundary_capability::tests::declared_level_never_exceeds_evaluated_when_sample_exists -- --nocapture
cargo test --manifest-path in-cli/Cargo.toml --lib language_gates::tests::declared_level_never_exceeds_evaluated_for_mandatory_samples -- --nocapture

echo "language gate checks ok"