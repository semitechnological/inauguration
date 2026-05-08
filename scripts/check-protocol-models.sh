#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

cargo run --quiet --manifest-path in-cli/Cargo.toml --bin protocol-gen -- "$ROOT"

git diff --exit-code -- \
  in-cli/src/hotreload/generated_protocol.rs \
  runtime/swift-preview-host/Sources/SwiftPreviewHost/GeneratedProtocol.swift
