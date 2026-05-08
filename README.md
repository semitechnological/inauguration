# inauguration

`inauguration` is fast developer toolchain for Swift projects: incremental compiler experimentation, SIL analysis, and low-latency SwiftUI hot reload.

## What it is

- **Compiler workspace** for Swift front-end experimentation (OCaml parser/checker + Rust pipeline).
- **Runtime hot reload system** for SwiftUI (daemon + preview host bridge).
- **CLI (`in`)** for build, test, benchmark, plugin installation, and daily dev workflows.

The `in` binary embeds the Rust hybrid pipeline and dispatch; **Apple’s `swift` toolchain is still required** for `swift build` emit, package resolution, and host-specific compilation.

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

## `in build` and SwiftPM staging (macOS/Linux)

After a successful **`swift build`** for a package (when the target path lives under a directory that contains `Package.swift`), `in` creates predictable links under the package root:

- **`.build/bin`**: runnable products — executable files (excluding typical non-binaries such as `.dylib`, `.a`, `.swiftmodule`, `.json`, `.txt`) plus **`.app`** bundles.
- **`.build/artifacts`**: auxiliary outputs such as **`.xctest`**, **`.dSYM`**, **`.bundle`**, **`.product`**, and loose **`.plist`** files (e.g. entitlements).

Those directories are emptied on each `in build`, then repopulated with **symlinks** to the real SwiftPM layout from `swift build --show-bin-path`. SwiftPM plumbing (e.g. `Modules`, `ModuleCache`, `index`, `description.json`) is not staged. On non-Unix targets, staging is skipped and the success line points at the Swift products directory only.

## Install CLI (pick one)

Recommended order:

**1. Wax (Homebrew-compatible parity)**

```bash
wax tap semitechnological/tap
wax install inauguration
```

**2. Homebrew tap**

```bash
brew tap semitechnological/tap
brew install inauguration
```

**3. Install script (GitHub release tarball or build from clone)**

From a clone of this repository (detects `in-cli` with `name = "inauguration"`, runs `cargo build --release` inside `in-cli` unless `IN_USE_RELEASE=1`):

```bash
./install.sh
```

Release assets match [`.github/workflows/release.yml`](.github/workflows/release.yml) (e.g. `in-macos-aarch64.tar.gz`, `in-linux-x86_64.tar.gz`). Override install dir with **`IN_INSTALL_DIR`**, pin a tag with **`IN_VERSION`**, prefer release when working in a tree with **`IN_USE_RELEASE=1`**. A thin wrapper remains at [`scripts/install.sh`](scripts/install.sh) and delegates to `./install.sh`.

**4. Build from source in this workspace**

```bash
cargo build --release --manifest-path in-cli/Cargo.toml
# binary: in-cli/target/release/in
```

Equivalent to `cargo install --path in-cli --bin in --force` for a local path install.

**Crates.io:** `cargo publish` from `in-cli` is not supported until the workspace `hybrid-*` crates are published with version requirements; the CLI is distributed via GitHub releases, Homebrew, Wax, and `./install.sh`.

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

The shell driver exports defaults when unset: **`BENCH_RUNS=3`**, **`BENCH_WARMUP_RUNS=1`**. Override via the environment (`BENCH_RUNS`, `BENCH_WARMUP_RUNS`).

Writes:

- `docs/benchmarks/swift-vs-in.md`
- `docs/benchmarks/swift-vs-in.json`

The markdown benchmark report includes:
- `Benchmark Environment` (host specs + tool versions) for reproducibility.
- `Easy Copy/Paste` table with exactly 3 columns (`Example`, `swift build(ms)`, `in(ms)`) for quick sharing.

### Latest Benchmark Snapshot

Generated (UTC): `2026-05-08T16:17:59Z`

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
| `aurorality/examples/counter` | 789.62 | 5.14 |
| `aurorality/examples/basic` | 763.95 | 4.85 |
| `aurorality/examples/hyperchat` | 320.13 | 5.12 |

Swift compiler sources (`swift` toolchain package) vs `in`:

| Example | swift build(ms) | in(ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 321.65 | 5.15 |

`SwiftCompilerSources` is a library product package, so SwiftPM may place library products under its build tree; **`in build` still stages runnable and artifact-like outputs** into `.build/bin` and `.build/artifacts` when present in the `swift build --show-bin-path` directory.

## Acknowledgements

- [brisk by plyght](https://github.com/plyght/brisk) for build orchestration patterns and command UX inspiration.
