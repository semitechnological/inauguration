# rust-driver

Concurrent orchestrator for hybrid MVP.

## Crates

- `hybrid-core`: shared event/task/metrics types
- `hybrid-scheduler`: cancellable, debounce-ready build wave scheduler
- `hybrid-sil`: textual SIL parser + first analysis transform
- `hybrid-pipeline`: task execution and OCaml artifact summary bridge
- `hybrid-cli`: thin command entry for local runs

## Validation

```bash
cargo fmt --all --check
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo test --all
```
