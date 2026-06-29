#!/bin/bash
cd compiler/rust-driver
cargo test -p hybrid-pipeline --lib tests::compiler_returns_error_for_unsupported_target
