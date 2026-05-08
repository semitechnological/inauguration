# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context **`swift build`** (SwiftPM reference — Apple toolchain), and **`in build`** default (**native hybrid pipeline only**, no SwiftPM).
**in** column = inauguration compile path (scheduler + SIL passes today); **swift build** = legacy SwiftPM baseline until native codegen fully replaces it.
**hybrid-cli** matches the native wave harness without the **`in`** CLI wrapper overhead.
Wall times: **median** over `3` timed runs; **min–max** across those runs shown in parentheses next to medians (easy tables) or inline (detail table).

## Benchmark Environment

- Generated (UTC): `2026-05-08T17:47:35Z`
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
| `aurorality/examples/counter` | 783.30 (771.19–787.43) | 1362.89 (1355.16–1390.69) |
| `aurorality/examples/basic` | 841.17 (800.40–841.66) | 1513.38 (1394.81–1695.28) |
| `aurorality/examples/hyperchat` | 351.80 (348.86–447.35) | 1421.84 (1405.69–1723.92) |

## Swift Toolchain (Compiler Sources) Benchmark

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 410.68 (393.83–412.89) | 613.21 (576.84–631.83) |

| Example | swiftc med (min–max) | SwiftPM med (min–max) | in native med (min–max) | hybrid-cli med (min–max) | native÷SwiftPM | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `aurorality/examples/counter` | 147.72 (146.48–169.94) | 783.30 (771.19–787.43) | 1362.89 (1355.16–1390.69) | 5.43 (5.23–5.58) | 1.740 | 1356.723 | 6.164 | 1357.460 | swift-frontend-stage | ✅ | ✅ |
| `aurorality/examples/basic` | 147.17 (140.65–157.94) | 841.17 (800.40–841.66) | 1513.38 (1394.81–1695.28) | 5.54 (5.22–6.64) | 1.799 | 1506.350 | 7.034 | 1507.846 | swift-frontend-stage | ✅ | ✅ |
| `aurorality/examples/hyperchat` | 9513.92 (9318.64–9657.45) | 351.80 (348.86–447.35) | 1421.84 (1405.69–1723.92) | 5.39 (5.15–5.65) | 4.042 | 0.000 | 1421.843 | 1416.452 | driver-overhead | ✅ | ❌ |
