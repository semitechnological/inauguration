# AGENTS

Contributor guide for humans and coding agents working in `inauguration`.

## Mission

Ship an **ultrafast** compiler for **general object-oriented and C-type** languages: one **Core IR** and SIL pipeline, many fronts (C family via Tree-sitter, **`.in` / `.icore`**, Rust/Go/V lowers, Swift via **`swiftc`** and the in-tree **subset**). Improve these layers together:

1. **`in-cli`**: **`lower_core`**, **`core_ir`**, **`compiler::tree_front`** (Tree-sitter + trivial C/C++/ObjC++ bodies where wired), **`parser_registry`**, **`swift_subset`** + **`native_swift_sil`**, **`sil_emit`**, embedded **`hybrid_*`** crates, **`in`** CLI, hotreload daemon, **`protocol-gen`**
2. **`compiler/rust-driver`**: pipeline, orchestration, **`hybrid-sil`** and related crates, batch path performance
3. **`runtime/*`**: SwiftUI reload latency, preview host apply semantics, thin daemon wrappers and tests

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

Use an `in` binary built from this repo. From a checkout, run **`in update`** (alias **`in self-update`**) to reinstall via `cargo install --path in-cli --locked` (honours **`IN_INSTALL_DIR`** like `./install.sh`). Outside a checkout, `in update` falls back to remote `install.sh` from `https://raw.githubusercontent.com/${IN_REPO:-semitechnological/inauguration}/master/install.sh` on Unix hosts. You can still use `./install.sh` or `cargo install --path in-cli --force` manually. A stale globally installed `in` (older than `in-cli` in your tree) can fail mid-suite with `No such file or directory` because `in test` must match the workspace layout.

Set **`IN_TEST_SKIP_SWIFT=1`** (or **`true`**, case-insensitive) to skip the `runtime/swift-preview-host` `swift package clean` and `swift test` steps during `in test` when Swift is unavailable; all other test steps still run.

If touching benchmarks or runtime timing, also run:

```bash
./scripts/bench-swift.sh
in bench
```

## Rust `protocol-gen` vs V

- **`protocol-gen` (Rust, `in-cli`)**: canonical checked-in codegen for `PatchType` / Swift `GeneratedWirePatchType` from `shared/protocol/events.schema.json`. CI runs **`scripts/check-protocol-models.sh`** (Rust generator + `git diff`).
- **V**: benchmark driver **`scripts/bench_swift.v`** (via **`scripts/bench-swift.sh`**: `v -gc none run …/bench_swift.v`) plus optional tools such as **`shared/protocol/generate_models.v`**.

## Code ownership map

- `in-cli/src/swift_subset.rs`: subset parse + check + artifact JSON (contract: [docs/architecture/subset-grammar.md](docs/architecture/subset-grammar.md))
- `in-cli/src/native_swift_sil.rs`: line filter + bridge into subset emit when **`IN_NATIVE_SWIFT_SIL`** is set
- `in-cli/src/sil_emit.rs`: Swift source discovery, **`swiftc`** invocation, merge of textual SIL primaries
- `in-cli/src/in_lang_parse.rs`, `lower_core.rs`, `core_ir.rs`: **`.in`** and unified Core IR module
- `in-cli/src/parser_registry.rs`: extension and shebang resolution to parser ids
- `in-cli/src/compiler/*`: multi-front driver, **icore** JSON → Core IR, **tree_front** (Tree-sitter polyglot + dedicated fronts)
- `compiler/rust-driver/crates/pipeline`: stage model + artifact ingestion
- `compiler/rust-driver/crates/sil`: SIL analysis/transforms
- `runtime/hotreload-daemon`: watch/decision/metrics loop (implementation lives in `in-cli`; crate is wrapper + tests)
- `runtime/swift-preview-host`: Swift package; patch apply semantics against generated protocol models
- `in-cli` (remainder): **`main`**, plugins, **`in test`**, preview clients, hybrid embedding

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
