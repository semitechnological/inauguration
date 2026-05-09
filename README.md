# inauguration

`inauguration` is fast developer toolchain for Swift projects: incremental compiler experimentation, SIL analysis, and low-latency SwiftUI hot reload.

## What it is

- **Compiler workspace** for experimenting with Swift tooling (pipeline, SIL passes, small frontend checks) and **multi-front experimentation** (Swift, line-oriented **`.in`**, Core IR → stub SIL — see [`.in` language](docs/architecture/in-language.md) and [multi-frontend IR](docs/architecture/multi-frontend-ir.md)).
- **Hot reload** path for SwiftUI: daemon plus preview host bridge.
- **`in` CLI** for daily workflows: build, dev loop, tests, benchmarks, plugins.

The published **`in`** binary bundles the hybrid compile wave (native default for **`in build`**), hotreload daemon, and socket-based dev preview. **`in dev`** uses a lightweight client by default; pass **`--preview-client swift`** when you want the SwiftPM preview host. Use **`in build --swiftpm`** only when you need SwiftPM **`swift build`** plus staging as a fallback alongside the native pipeline.

Hotreload wire formats live under **`shared/protocol`**; regenerators and benchmark helpers live in **`in-cli`** and **`scripts`** (see **`scripts/check-protocol-models.sh`** and **`./scripts/bench-swift.sh`**).

**Protocol models:** Rust **`protocol-gen`** (`cargo run --manifest-path in-cli/Cargo.toml --bin protocol-gen`) is the **canonical checked-in codegen** from `shared/protocol/events.schema.json`. **V** is retained for **`scripts/bench_swift.v`** (Swift vs `in` timings via **`./scripts/bench-swift.sh`**) and optional **`shared/protocol/generate_models.v`** minor-tool parity (same emitted Rust/Swift as `protocol-gen`; headers identify the V generator).

## Repository layout

- `compiler/rust-driver`: orchestrator, pipeline, SIL analysis, batch compile path.
- `in-cli`: **`in`** CLI, hybrid pipeline sources, hotreload daemon, protocol regeneration.
- `runtime/hotreload-daemon`: thin `cargo run` wrapper + integration tests (daemon sources live in `in-cli`, embedded into `in`).
- `runtime/swift-preview-host`: Swift package receiving and applying reload envelopes.
- `plugins/registry`: installable project accelerators (aurorality, crepuscularity).
- `scripts`: operational scripts (dev loop, compiler benchmark harness).
- `docs/architecture`: architecture and local runbooks ([interop roadmap](docs/architecture/interop-roadmap.md), [native Swift master plan](docs/architecture/native-swift-master-plan.md), [`.in` language](docs/architecture/in-language.md), [multi-frontend IR](docs/architecture/multi-frontend-ir.md)). Hybrid `in-cli` ↔ `rust-driver` mirror: [docs/contributing-hybrid-mirror.md](docs/contributing-hybrid-mirror.md).
- `docs/benchmarks`: benchmark reports and generated comparison artifacts.
- `apps/native-subset-sample`: tiny Swift-shaped sample for the **in-tree** subset compiler (no **`swiftc`** when **`IN_NATIVE_SWIFT_SIL=only`**).
- `apps/in-sample`: minimal **`.in` v0** module (struct + helper **`fn`** + **`fn main`**); run **`./scripts/check-in-lang-sample.sh`** (**`in build --parser in --path …`**) — no **`swiftc`**.

## `in build` and SwiftPM staging (macOS/Linux)

Default **`in build`** runs the native hybrid pipeline: it gathers Swift sources (single file, **`Sources/`** tree when a **`Package.swift`** is present), emits **textual SIL**, then applies inauguration SIL passes. By default SIL comes from **`swiftc -emit-sil`** (toolchain on **`PATH`**, or override with **`IN_SWIFTC`**).

**`.in` v0 (no `swiftc`):** **`in build --path apps/in-sample/hello.in --module-id App`** — default **`--parser auto`** selects the `.in` front from the file extension; use **`--parser in`** or **`IN_PARSER=in`** to force. For a **single existing file**, a first line **`#!in parser=in`** selects the `.in` front even when the extension is not **`.in`**; **`#!in parser=auto`** defers to the usual **`auto`** rules (extension and **`IN_PARSER`**). **`--parser in`** still forces the `.in` front regardless. Sample + script: **`apps/in-sample/hello.in`**, **`./scripts/check-in-lang-sample.sh`** (CI job **`in-lang-sample`**). Core IR → same stub SIL path as the Swift subset ([`docs/architecture/in-language.md`](docs/architecture/in-language.md)). Structs may declare **inline fields** on the `struct` line, e.g. **`struct Session { Int id; String label }`** (semicolon-separated **`Type name`** segments inside `{` … `}`).

**In-tree subset (Rust, no `swiftc`):** set **`IN_NATIVE_SWIFT_SIL=try`** to try the line-oriented **`swift_subset`** front first and fall back to **`swiftc`** when the source is not a valid subset. Use **`IN_NATIVE_SWIFT_SIL=only`** to require the in-tree path (CI or hermetic checks). Contracted syntax: **[docs/architecture/subset-grammar.md](docs/architecture/subset-grammar.md)**; sample **`apps/native-subset-sample/App.swift`**; local check **`./scripts/check-native-subset-sample.sh`**. With **`--swiftpm`**, **`in`** additionally runs **`swift build`** and stages outputs for runnable artifacts.

After a successful **`swift build`** inside that optional step (when the target path resolves under a directory that contains `Package.swift`), **`in`** creates predictable links under the package root:

- **`.build/bin`**: runnable products — executable files (excluding typical non-binaries such as `.dylib`, `.a`, `.swiftmodule`, `.json`, `.txt`) plus **`.app`** bundles.
- **`.build/artifacts`**: auxiliary outputs such as **`.xctest`**, **`.dSYM`**, **`.bundle`**, **`.product`**, and loose **`.plist`** files (e.g. entitlements).

Those directories are emptied on each **`in build --swiftpm`**, then repopulated with **symlinks** to the real SwiftPM layout from **`swift build --show-bin-path`**. SwiftPM plumbing (e.g. `Modules`, `ModuleCache`, `index`, `description.json`) is not staged. On non-Unix targets, staging is skipped.

## Install CLI (pick one)

Recommended order:

**1. crates.io**

```bash
cargo install inauguration
```

Installs the **`in`** binary as a single package.

**2. Wax (Homebrew-compatible parity)**

```bash
wax tap semitechnological/tap
wax install inauguration
```

**3. Homebrew tap**

```bash
brew tap semitechnological/tap
brew install inauguration
```

**4. Install script (GitHub release tarball or build from clone)**

From a clone of this repository (detects `in-cli` with `name = "inauguration"`, runs `cargo build --release` inside `in-cli` unless `IN_USE_RELEASE=1`):

```bash
./install.sh
```

Release assets match [`.github/workflows/release.yml`](.github/workflows/release.yml) (e.g. `in-macos-aarch64.tar.gz`, `in-linux-x86_64.tar.gz`). Override install dir with **`IN_INSTALL_DIR`**, pin a tag with **`IN_VERSION`**, prefer release when working in a tree with **`IN_USE_RELEASE=1`**. A thin wrapper remains at [`scripts/install.sh`](scripts/install.sh) and delegates to `./install.sh`.

**5. Build from source in this workspace**

```bash
cargo build --release --manifest-path in-cli/Cargo.toml
# binary: in-cli/target/release/in
```

Equivalent to `cargo install --path in-cli --bin in --force` for a local path install (matches the crates.io package layout).

Publishing: **`inauguration`** crate ships from **`in-cli`**; sources stay aligned with **`compiler/rust-driver`**.

## Core commands

```bash
in build
in build --parser in --path apps/in-sample/hello.in --module-id App
in build --swiftpm --path ../aurorality/examples   # native pipeline + SwiftPM emit + staging
in dev
in dev --preview-client swift       # Swift preview host + SwiftUI path
in run
in ocaml path/to/File.swift         # experimental Swift subset check → JSON
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
cd in-cli && cargo test
cd runtime/swift-preview-host && swift build -Xswiftc -warnings-as-errors && swift test
cd runtime/hotreload-daemon && cargo test
./scripts/check-protocol-models.sh # runs protocol-gen (Rust) then git diff
./scripts/check-native-subset-sample.sh # IN_NATIVE_SWIFT_SIL=only; no swiftc
./scripts/check-in-lang-sample.sh       # in build --parser in; no swiftc
# Optional: same outputs via V (header comment differs); not run in CI.
v -gc none run shared/protocol/generate_models.v "$(pwd)"   # omit "$(pwd)" to walk up to repo root
```

CI stays on **`protocol-gen` (Rust)** via **`scripts/check-protocol-models.sh`**.

## Benchmarking

```bash
./scripts/bench-swift.sh
```

[`scripts/bench-swift.sh`](scripts/bench-swift.sh) exports **`BENCH_ROOT`** to the repository root and runs **`v -gc none run "$BENCH_ROOT/scripts/bench_swift.v`**. The shell driver exports defaults when unset: **`BENCH_RUNS=3`**, **`BENCH_WARMUP_RUNS=1`**. Override via the environment (`BENCH_RUNS`, `BENCH_WARMUP_RUNS`). If bare **`v run`** fails with missing **`gc.h`**, use **`v -gc none`** (as this script does) or install Boehm **`gc`** development headers.

Writes:

- `docs/benchmarks/swift-vs-in.md`
- `docs/benchmarks/swift-vs-in.json`

The markdown report records host/tool versions and tables where each cell is **median wall ms** over `BENCH_RUNS` timed iterations, with **min–max** in parentheses. **`in build`** timings use the **native** pipeline only (default CLI); **SwiftPM `swift build`** is a separate baseline column until native codegen fully replaces the Apple driver. **`swiftc -typecheck`** is a single-file probe (often fails on SwiftUI-heavy files; harness continues). Details: [`docs/benchmarks/swift-vs-in.md`](docs/benchmarks/swift-vs-in.md) (+ [`swift-vs-in.json`](docs/benchmarks/swift-vs-in.json)).

### Latest Benchmark Snapshot

Copied from **`docs/benchmarks/swift-vs-in.md`** Easy Copy tables after **`./scripts/bench-swift.sh`**. Re-run script and paste here + commit when refreshing.

Median over three runs; parentheses = min–max on those same runs.

Generated (UTC): `2026-05-08T17:47:35Z` · macOS · Apple M5 Pro · Swift 6.3 · rustc 1.94.1 · V 0.5.0

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `aurorality/examples/counter` | 783.30 (771.19–787.43) | 1362.89 (1355.16–1390.69) |
| `aurorality/examples/basic` | 841.17 (800.40–841.66) | 1513.38 (1394.81–1695.28) |
| `aurorality/examples/hyperchat` | 351.80 (348.86–447.35) | 1421.84 (1405.69–1723.92) |

Swift compiler sources package vs **`in`** native:

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 410.68 (393.83–412.89) | 613.21 (576.84–631.83) |

Staging under **`.build/bin`** / **`.build/artifacts`** applies only when using **`in build --swiftpm`** (SwiftPM emit step).

## Acknowledgements

- [brisk by plyght](https://github.com/plyght/brisk) for build orchestration patterns and command UX inspiration.
