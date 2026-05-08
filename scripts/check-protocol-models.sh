#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

v -gc none run shared/protocol/generate_models.v

git diff --exit-code -- \
  runtime/hotreload-daemon/src/generated_protocol.rs \
  runtime/swift-preview-host/Sources/SwiftPreviewHost/GeneratedProtocol.swift
