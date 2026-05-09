# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context **`swift build`** (SwiftPM reference — Apple toolchain), and **`in build`** default (**native hybrid pipeline only**, no SwiftPM).
**in** column = inauguration compile path (scheduler + SIL passes today); **swift build** = legacy SwiftPM baseline until native codegen fully replaces it.
**hybrid-cli** matches the native wave harness without the **`in`** CLI wrapper overhead.
Wall times: **median** over `3` timed runs; **min–max** across those runs shown in parentheses next to medians (easy tables) or inline (detail table).

## Benchmark Environment

- Generated (UTC): `2026-05-09T02:42:45Z`
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
| `aurorality/examples/counter` | 835.77 (807.60–1007.26) | 1486.51 (1421.22–1536.50) |
| `aurorality/examples/basic` | 773.02 (756.07–841.40) | 1403.61 (1358.42–1454.95) |
| `aurorality/examples/hyperchat` | 349.00 (344.37–393.56) | 1433.38 (1415.83–1448.71) |

## Swift Toolchain (Compiler Sources) Benchmark

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 352.07 (340.88–357.04) | 521.68 (514.37–619.41) |

| Example | swiftc med (min–max) | SwiftPM med (min–max) | in native med (min–max) | hybrid-cli med (min–max) | native÷SwiftPM | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `aurorality/examples/counter` | 158.73 (158.15–182.21) | 835.77 (807.60–1007.26) | 1486.51 (1421.22–1536.50) | 5.38 (5.36–6.98) | 1.779 | 1478.132 | 8.375 | 1481.131 | swift-frontend-stage | ✅ | ✅ |
| `aurorality/examples/basic` | 145.50 (142.32–149.73) | 773.02 (756.07–841.40) | 1403.61 (1358.42–1454.95) | 5.11 (5.09–5.95) | 1.816 | 1397.578 | 6.032 | 1398.498 | swift-frontend-stage | ✅ | ✅ |
| `aurorality/examples/hyperchat` | 9605.26 (9241.43–9691.60) | 349.00 (344.37–393.56) | 1433.38 (1415.83–1448.71) | 5.97 (5.45–6.40) | 4.107 | 0.000 | 1433.380 | 1427.405 | driver-overhead | ✅ | ❌ |
