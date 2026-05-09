# Swift Compiler vs in Pipeline Benchmark

Measured with: **`swiftc -typecheck`** on a single file when there is no local `Package.swift`; when there is a package, **`scripts/swiftc-bench-typecheck.sh`** (same Sources + Generated inputs and Clang flags idea as `in-cli` **`sil_emit`**) after a timed **`swift build`**. Also package-context **`swift build`** (SwiftPM reference) and **`in build`** default (**native hybrid pipeline only**, no SwiftPM).
**in** column = inauguration compile path (scheduler + SIL passes today); **swift build** = legacy SwiftPM baseline until native codegen fully replaces it.
**hybrid-cli** matches the native wave harness without the **`in`** CLI wrapper overhead.
Wall times: **median** over `3` timed runs; **min–max** across those runs shown in parentheses next to medians (easy tables) or inline (detail table).

## Benchmark Environment

- Generated (UTC): `2026-05-09T14:19:19Z`
- Host OS: `Darwin`
- Kernel: `25.4.0`
- CPU: `Apple M5 Pro`
- Memory: `51539607552`
- Swift: `swift-driver version: 1.148.6 Apple Swift version 6.3 (swiftlang-6.3.0.123.5 clang-2100.0.123.102)
Target: arm64-apple-macosx26.0`
- Rustc: `rustc 1.94.1 (e408947bf 2026-03-25)`
- Cargo: `cargo 1.94.1 (29ea6fb6a 2026-03-24)`
- V: `V 0.5.0 e2f5d6c`
- BENCH_RUNS: `3`
- BENCH_WARMUP_RUNS: `1`
- in binary: `/Users/undivisible/projects/inauguration/in-cli/target/debug/in`
- hybrid-cli binary: `/Users/undivisible/projects/inauguration/compiler/rust-driver/target/debug/hybrid-cli`

## Easy Copy/Paste

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `aurorality/examples/counter` | 984.37 (952.47–1004.23) | 548.29 (540.68–578.89) |
| `aurorality/examples/basic` | 943.56 (936.91–1000.56) | 559.43 (541.97–608.84) |
| `aurorality/examples/hyperchat` | 433.33 (386.77–614.14) | 4334.97 (3155.60–4571.39) |

## SwiftPM Package (Preview Host) Benchmark

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `runtime/swift-preview-host` | 681.30 (647.08–781.19) | 771.99 (725.52–774.55) |

| Example | swiftc med (min–max) | SwiftPM med (min–max) | in native med (min–max) | hybrid-cli med (min–max) | native÷SwiftPM | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `aurorality/examples/counter` | 395.13 (395.11–414.95) | 984.37 (952.47–1004.23) | 548.29 (540.68–578.89) | 7.05 (6.87–7.17) | 0.557 | 538.847 | 9.447 | 541.248 | win | ✅ | ✅ |
| `aurorality/examples/basic` | 402.42 (371.62–412.28) | 943.56 (936.91–1000.56) | 559.43 (541.97–608.84) | 6.84 (6.06–7.15) | 0.593 | 550.636 | 8.792 | 552.593 | win | ✅ | ✅ |
| `aurorality/examples/hyperchat` | 2326.91 (1966.04–3817.95) | 433.33 (386.77–614.14) | 4334.97 (3155.60–4571.39) | 15.00 (8.19–15.83) | 10.004 | 4319.776 | 15.194 | 4319.970 | swift-frontend-stage | ✅ | ✅ |
