# inauguration

Hybrid MVP for Swift reimplementation + SwiftUI hot reload.

## Layout

- `compiler/rust-driver`: concurrent orchestrator, SIL pipeline, CLI
- `compiler/ocaml-front`: Swift subset parser + minimal type checker
- `runtime/hotreload-daemon`: file watcher + patch planner + metrics
- `runtime/swift-preview-host`: Swift package host that receives reload patches
- `apps/sample-swiftui`: sample app for edit-to-preview benchmarks
- `vendor/swift`: shallow clone of `swiftlang/swift`
- `docs/architecture`: architecture + interfaces
- `docs/benchmarks`: week-6 benchmark report template

## Quickstart

```bash
cd compiler/rust-driver
cargo test --all
cargo clippy --all-targets --all-features --locked -- -D warnings
cargo fmt --check
```

```bash
cd compiler/ocaml-front
opam install . --deps-only --with-test
dune runtest
```

```bash
cd runtime/swift-preview-host
swift build
swift build -warnings-as-errors
swift test
```

## Notes

- Apple-first runtime path for SwiftUI hot reload.
- Compiler subset intentionally narrow for deterministic milestone.
- Built on top of ideas and workflows from `brisk`.

## MVP Loop

Use `docs/architecture/local-mvp-runbook.md` to run daemon + preview host client and capture patch metrics.

## in CLI

Install local binary:

```bash
cargo install --path in-cli --bin in --force
```

Commands:

```bash
in build
in build --path ../aurorality/examples
in dev
in run
in test
in doctor
in plugin list
in plugin install aurorality
in plugin run aurorality --target ../aurorality
```

## Acknowledgements

- `brisk` for build orchestration patterns and developer workflow inspiration.
