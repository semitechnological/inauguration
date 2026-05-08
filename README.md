# inauguration

`inauguration` is fast developer toolchain for Swift projects: incremental compiler experimentation, SIL analysis, and low-latency SwiftUI hot reload.

## What it is

- **Compiler workspace** for Swift front-end experimentation (OCaml parser/checker + Rust pipeline).
- **Runtime hot reload system** for SwiftUI (daemon + preview host bridge).
- **CLI (`in`)** for build, test, benchmark, plugin installation, and daily dev workflows.

## Repository layout

- `compiler/rust-driver`: concurrent orchestrator, pipeline, SIL analysis, batch compile path.
- `compiler/ocaml-front`: Swift subset front-end, diagnostics, artifact emission.
- `runtime/hotreload-daemon`: watcher, patch/restart supervisor, metrics emitter.
- `runtime/swift-preview-host`: Swift package receiving and applying reload envelopes.
- `in-cli`: user-facing command line binary (`in`).
- `plugins/registry`: installable project accelerators (aurorality, crepuscularity).
- `scripts`: operational scripts (dev loop, compiler benchmark harness).
- `docs/architecture`: architecture and local runbooks.
- `docs/benchmarks`: benchmark reports and generated comparison artifacts.
- `vendor/swift`: local clone of `swiftlang/swift` for architecture parity reference.

## Install CLI

```bash
cargo install --path in-cli --bin in --force
```

Wax (Homebrew-compatible parity):

```bash
wax tap semitechnological/tap
wax install inauguration
```

Homebrew tap (fallback):

```bash
brew tap semitechnological/tap
brew install inauguration
```

Direct install script:

```bash
./scripts/install.sh
```

## Core commands

```bash
in build
in build --path ../aurorality/examples
in dev
in run
in test
in doctor
in bench
```

## Plugin commands

```bash
in plugin list
in plugin install aurorality
in plugin install crepuscularity
in plugin run aurorality --target ../aurorality
```

## Validation commands

```bash
cd compiler/rust-driver && cargo test --all
cd compiler/ocaml-front && eval "$(opam env --switch=default)" && dune runtest
cd runtime/swift-preview-host && swift build -Xswiftc -warnings-as-errors && swift test
cd runtime/hotreload-daemon && cargo test
./scripts/check-protocol-models.sh
```

## Benchmarking

```bash
./scripts/bench-swift.sh
```

Useful knobs:

```bash
BENCH_WARMUP_RUNS=1 BENCH_RUNS=3 ./scripts/bench-swift.sh
```

Writes:

- `docs/benchmarks/swift-vs-in.md`
- `docs/benchmarks/swift-vs-in.json`

The markdown benchmark report includes:
- `Benchmark Environment` (host specs + tool versions) for reproducibility.
- `Easy Copy/Paste` table with exactly 3 columns (`Example`, `swift build(ms)`, `in(ms)`) for quick sharing.

### Latest Benchmark Snapshot

Generated (UTC): `2026-05-08T16:01:33Z`

Environment:
- OS: `macOS 26.5`
- CPU: `Apple M5 Pro`
- Memory: `48 GB`
- Swift: `6.3`
- Rustc: `rustc 1.94.1 (e408947bf 2026-03-25)`
- Cargo: `cargo 1.94.1 (29ea6fb6a 2026-03-24)`
- V: `V 0.5.0 e2f5d6c`

| Example | swift build(ms) | in(ms) |
|---|---:|---:|
| `aurorality/examples/counter` | 844.15 | 5.85 |
| `aurorality/examples/basic` | 812.48 | 5.95 |
| `aurorality/examples/hyperchat` | 354.78 | 5.51 |

Swift compiler sources (`swift` toolchain package) vs `in`:

| Example | swift build(ms) | in(ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 342.05 | 6.03 |

`SwiftCompilerSources` is a library product package, so `in build` emits library artifacts in `.build/.../debug` rather than a standalone executable binary.

## Acknowledgements

- `brisk` for build orchestration patterns and command UX inspiration.
