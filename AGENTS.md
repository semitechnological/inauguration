# AGENTS

Contributor guide for humans and coding agents working in `inauguration`.

## Mission

Ship faster Swift developer workflows by improving three core layers together:

1. `in-cli` Swift subset front (`swift_subset`) + CLI workflows
2. `compiler/rust-driver` (pipeline/orchestration/perf)
3. `runtime/*` (reload latency and reliability)

## Working rules

- Prefer small, composable changes with tests in same commit.
- Keep terminal output useful: timings, reason codes, clear failures.
- Do not break `in test`.
- Preserve CLI ergonomics: sensible defaults first, flags second.
- When adding concurrency, keep behavior deterministic and observable.

## Required checks before push

Run all:

```bash
in test
```

If touching benchmarks or runtime timing, also run:

```bash
./scripts/bench-swift.sh
in bench
```

## Rust `protocol-gen` vs V

- **`protocol-gen` (Rust, `in-cli`)**: canonical checked-in codegen for `PatchType` / Swift `GeneratedWirePatchType` from `shared/protocol/events.schema.json`. CI runs **`scripts/check-protocol-models.sh`** (Rust generator + `git diff`).
- **V**: benchmark driver **`scripts/bench_swift.v`** (via **`scripts/bench-swift.sh`**: `v -gc none run …/bench_swift.v`) plus optional tools such as **`shared/protocol/generate_models.v`**.

## Code ownership map

- `in-cli/src/swift_subset.rs`: Swift subset parser/checker/artifact JSON
- `in-cli/src/compiler/*`: multi-front driver, **icore** JSON → Core IR
- `compiler/rust-driver/crates/pipeline`: stage model + artifact ingestion
- `compiler/rust-driver/crates/sil`: SIL analysis/transforms
- `runtime/hotreload-daemon`: watch/decision/metrics loop
- `runtime/swift-preview-host`: patch apply semantics
- `in-cli`: user workflow and plugin surface

## Plugin policy

- Built-in plugins live in `plugins/registry/*.sh`.
- Plugins must be safe on repeated runs (idempotent where practical).
- Plugins should improve one project profile clearly (e.g. aurorality, crepuscularity).

## Performance policy

- Prefer default-on improvements over extra toggles.
- Expose stage timing whenever behavior gets slower.
- Keep benchmark outputs checked in under `docs/benchmarks` when meaningful.

## Commit style

- Use concise imperative subject.
- Body should describe why change matters for speed, correctness, or UX.
