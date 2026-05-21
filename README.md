# inauguration

`inauguration` targets an **ultrafast** compiler for **general object-oriented and C-family** languages: multiple parsers lower into one **Core IR**, then into **textual SIL** that **`hybrid_sil`** analyzes in Rust. **Swift** is a first shipping surface (**`swiftc`** emit, in-tree **Swift-shaped subset**, SwiftPM staging) alongside **`.in`**, **`.icore`**, **Tree-sitter** polyglot fronts, and bounded **Rust / Go / V** lowers. A **SwiftUI hot reload** daemon and **`in`** CLI wrap the same pipeline for day-to-day workflows.

## What it is

- **Compiler core** (`in-cli` + `compiler/rust-driver`): shared **Core IR** → **`lower_core`** → textual SIL → **`hybrid_sil`** passes; fronts include **C / C++ / ObjC++** and other Tree-sitter grammars (see [parser surface](docs/architecture/parser-surface.md)), **`.in`** / **`.icore`**, dedicated **Rust / Go / V** parsers, and Swift via **`swiftc`** or the **hermetic** **`IN_NATIVE_SWIFT_SIL`** path ([subset grammar](docs/architecture/subset-grammar.md)); see [multi-frontend IR](docs/architecture/multi-frontend-ir.md) and [`.in` language](docs/architecture/in-language.md).
- **`in` CLI** (crate **`inauguration`**, binary **`in`**): **`in build`**, **`in dev`**, **`in test`**, **`in bench`**, plugins, optional SwiftPM staging — [Core commands](#core-commands).
- **Hot reload** for SwiftUI: Unix-socket daemon in **`in`**, metrics, **`runtime/swift-preview-host`**, wire format under **`shared/protocol`**.
- **Ecosystem glue**: **`plugins/registry`**, **`docs-site`**, **`scripts`** and **`docs/benchmarks`** harnesses.

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
- `docs/architecture`: architecture and local runbooks ([interop roadmap](docs/architecture/interop-roadmap.md), [native Swift master plan](docs/architecture/native-swift-master-plan.md), [`.in` language](docs/architecture/in-language.md), [multi-frontend IR](docs/architecture/multi-frontend-ir.md), [parser surface / extension map](docs/architecture/parser-surface.md)). Hybrid `in-cli` ↔ `rust-driver` mirror: [docs/contributing-hybrid-mirror.md](docs/contributing-hybrid-mirror.md).
- `docs-site`: static **crepuscularity-web** site (Vercel-inspired light UI, **Instrument Sans**); build with **`./scripts/build-docs-site.sh`** (needs sibling **`../crepuscularity`** or **`crepus`** on `PATH`). Top-level **`docs/*.md` symlinks** expose nested guides to `crepus web build`.
- `docs/benchmarks`: benchmark reports and generated comparison artifacts.
- `apps/native-subset-sample`: tiny Swift-shaped sample for the **in-tree** subset compiler (no **`swiftc`** when **`IN_NATIVE_SWIFT_SIL=only`**).
- `apps/in-sample`: minimal **`.in`** modules (struct + helper **`fn`** + **`fn main`** with bounded body statements, plus an agent-native import/capability/extern binding sample); run **`./scripts/check-in-lang-sample.sh`** (**`in build --parser in --path …`**) — no **`swiftc`**.
- `apps/icore-sample`: **`.icore` JSON** Core IR modules: v1 empty bodies plus v2 bounded body JSON; run **`./scripts/check-icore-sample.sh`** (CI **`icore-sample`**) — no **`swiftc`**.

## `in build` and SwiftPM staging (macOS/Linux)

Default **`in build`** runs the native hybrid pipeline: it gathers Swift sources (single file; under **`Package.swift`**, the **`Sources/<target>/`** tree that contains the entry file so unrelated targets are not merged; plus **`Generated/`** when present), emits **textual SIL**, then applies inauguration SIL passes. By default SIL comes from **`swiftc -emit-sil`** (toolchain on **`PATH`**, or override with **`IN_SWIFTC`**). **`swiftc`** rejects duplicate **basenames** across primaries; the driver splits those emits and concatenates SIL fragments.

**Core IR fronts (no `swiftc`):** **`.in`** — **`in build --path apps/in-sample/hello.in --module-id App`**; default **`--parser auto`** uses the extension; **`--parser in`** or **`IN_PARSER=in`** force that front. `.in` also accepts explicit `import`, `capability`, and `extern <language> fn ...;` surface declarations; local relative `.in` imports merge declarations into Core IR, and `in agent` reports imports/externs/capabilities as machine-readable facts while extern calls still lower through Core IR graph facts. **`.icore` (JSON)** — **[`docs/architecture/general-compiler.md`](docs/architecture/general-compiler.md)**; **`in build --path apps/icore-sample/min.icore --module-id App`** or **`--parser icore`** / **`IN_PARSER=icore`**. `icoreVersion: 1` keeps declaration + empty-body compatibility; `icoreVersion: 2` accepts bounded body JSON for `let`, assignment, return, call expressions, identifiers, and int/string/bool literals. Dedicated fronts now exist for **Rust** (`.rs`, `syn` + `rustc` validation), **Go** (`.go`), and **V** (`.v` / `vlang` magic token), each lowering real top-level declarations plus a bounded statement subset (declarations/assignments/returns and control-flow markers), not full language semantics yet. Tree-sitter fronts now include bounded Java/Groovy body lowering; other tracked polyglot extensions still route through declaration extraction; ids without a wired grammar return an `.icore` hint. For a single file, **`#!in parser=in`**, **`#!in parser=icore`**, or **`#!in parser=<slug>`** selects the front. See **[`docs/architecture/parser-surface.md`](docs/architecture/parser-surface.md)** and **[`docs/architecture/general-compiler.md`](docs/architecture/general-compiler.md)** for the exact matrix and roadmap.

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
in build --parser in --path apps/in-sample/agent-native.in --module-id App
in build --swiftpm --path ../aurorality/examples   # native pipeline + SwiftPM emit + staging
in agent --path apps/in-sample/agent-native.in --parser in
in explain INAGENT020 --json
in fix --plan --json --path apps/in-sample/hello.in --parser in
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
./scripts/check-icore-sample.sh         # icore v1/v2 samples; no swiftc
# Optional: same outputs via V (header comment differs); not run in CI.
v -gc none run shared/protocol/generate_models.v "$(pwd)"   # omit "$(pwd)" to walk up to repo root
```

CI stays on **`protocol-gen` (Rust)** via **`scripts/check-protocol-models.sh`**.

## Benchmarking

```bash
./scripts/bench-swift.sh
```

[`scripts/bench-swift.sh`](scripts/bench-swift.sh) exports **`BENCH_ROOT`** to the repository root and runs **`v -gc none run "$BENCH_ROOT/scripts/bench_swift.v`**. The shell driver exports defaults when unset: **`BENCH_RUNS=3`**, **`BENCH_WARMUP_RUNS=1`**. Override via the environment (`BENCH_RUNS`, `BENCH_WARMUP_RUNS`). If bare **`v run`** fails with missing **`gc.h`**, use **`v -gc none`** (as this script does) or install Boehm **`gc`** development headers.

For SwiftPM examples, the harness times **`swiftc -typecheck`** via [`scripts/swiftc-bench-typecheck.sh`](scripts/swiftc-bench-typecheck.sh): same **`Sources/`** + **`Generated/`** inputs and Clang **`-Xcc`** / **`-I`** layout as **`in-cli`** **`sil_emit`** (`.build/.../debug/Modules`, local **`generated/`** / **`FFI/`**, dependency **`generated/`** from **`.build/workspace-state.json`**), so the **`swiftc`** column matches SIL emit instead of typechecking one primary file in isolation.

Writes:

- `docs/benchmarks/swift-vs-in.md`
- `docs/benchmarks/swift-vs-in.json`

The markdown report records host/tool versions and tables where each cell is **median wall ms** over `BENCH_RUNS` timed iterations, with **min–max** in parentheses. **`in build`** timings use the **native** pipeline only (default CLI); **SwiftPM `swift build`** is a separate baseline column until native codegen fully replaces the Apple driver. **`swiftc`** uses a single-file probe only when there is no enclosing **`Package.swift`**; otherwise see **`swiftc-bench-typecheck.sh`** above. Details: [`docs/benchmarks/swift-vs-in.md`](docs/benchmarks/swift-vs-in.md) (+ [`swift-vs-in.json`](docs/benchmarks/swift-vs-in.json)).

### Latest Benchmark Snapshot

Copied from **`docs/benchmarks/swift-vs-in.md`** Easy Copy tables after **`./scripts/bench-swift.sh`**. Re-run script and paste here + commit when refreshing.

Median over `BENCH_RUNS` timed iterations (default 3); parentheses = min–max on those same runs.

Generated (UTC): `2026-05-09T14:19:19Z` · macOS · Apple M5 Pro · Swift 6.3 · rustc 1.94.1 · V 0.5.0

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `aurorality/examples/counter` | 984.37 (952.47–1004.23) | 548.29 (540.68–578.89) |
| `aurorality/examples/basic` | 943.56 (936.91–1000.56) | 559.43 (541.97–608.84) |
| `aurorality/examples/hyperchat` | 433.33 (386.77–614.14) | 4334.97 (3155.60–4571.39) |

Second SwiftPM package (in-tree preview host; replaces vendor-only Swift compiler sources, which need a full Ninja Swift build and are not a portable **`in build`** probe):

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `runtime/swift-preview-host` | 681.30 (647.08–781.19) | 771.99 (725.52–774.55) |

**Reading the harness:** the detail table in [`docs/benchmarks/swift-vs-in.md`](docs/benchmarks/swift-vs-in.md) labels a **loss bucket**. On small SwiftUI examples it is usually **`swift-frontend-stage`**: almost all wall time is Apple **`swiftc`** emitting textual SIL before the in-tree scheduler runs. **`hybrid-cli`** in the same report stays on the order of **single-digit ms**, which matches “Rust wave + SIL parse” being cheap today compared to **`swiftc`**.

**Where to optimize the compiler next (highest leverage):**

1. **Shrink or bypass `swiftc` on the hot path** — cache SIL keyed by inputs, tighter incremental **`swiftc`** flags where sound, and widen **`swift_subset`** / **`IN_NATIVE_SWIFT_SIL=try`** so simple files never spawn **`swiftc`**.
2. **Failure-heavy UI sources** — when **`in build`** exits non-zero for a package, investigate early diagnostics vs work still done in **`sil_emit`** / driver so we do not pay full subprocess cost for unsupported shapes (see detail table **`in ok`** column).
3. **After emit is cheap** — then profile **`parse_textual_sil`**, debug-instruction stripping, and call-graph extraction in `in-cli` (today a few ms on these samples); **`compiler/rust-driver`** pipeline stages matter more for batch / driver-scale work.

**`in bench`** reads **`.brisk/hotreload/metrics/latest.ndjson`** (compile-check rows from **`in dev`**). Run it from the repo root after a dev session that produced metrics, or pass **`--metrics <path>`** when summarizing another NDJSON file.

Staging under **`.build/bin`** / **`.build/artifacts`** applies only when using **`in build --swiftpm`** (SwiftPM emit step).

## Acknowledgements

- [brisk by plyght](https://github.com/plyght/brisk) for build orchestration patterns and command UX inspiration.
