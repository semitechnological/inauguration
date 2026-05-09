# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context **`swift build`** (SwiftPM reference — Apple toolchain), and **`in build`** default (**native hybrid pipeline only**, no SwiftPM).
**in** column = inauguration compile path (scheduler + SIL passes today); **swift build** = legacy SwiftPM baseline until native codegen fully replaces it.
**hybrid-cli** matches the native wave harness without the **`in`** CLI wrapper overhead.
Wall times: **median** over `3` timed runs; **min–max** across those runs shown in parentheses next to medians (easy tables) or inline (detail table).

## Benchmark Environment

- Generated (UTC): `2026-05-09T13:40:10Z`
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
| `aurorality/examples/counter` | 977.06 (969.39–1027.76) | 1789.80 (1703.27–1803.60) |
| `aurorality/examples/basic` | 998.06 (960.92–1087.47) | 1887.60 (1753.75–2044.03) |
| `aurorality/examples/hyperchat` | 424.45 (408.78–441.66) | 1776.65 (1659.63–1871.78) |

## Swift Toolchain (Compiler Sources) Benchmark

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 441.80 (421.76–443.66) | 627.60 (626.41–656.08) |

| Example | swiftc med (min–max) | SwiftPM med (min–max) | in native med (min–max) | hybrid-cli med (min–max) | native÷SwiftPM | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `aurorality/examples/counter` | 208.10 (195.44–208.63) | 977.06 (969.39–1027.76) | 1789.80 (1703.27–1803.60) | 6.74 (6.68–8.38) | 1.832 | 1779.988 | 9.811 | 1783.060 | swift-frontend-stage | ✅ | ✅ |
| `aurorality/examples/basic` | 183.64 (178.20–189.88) | 998.06 (960.92–1087.47) | 1887.60 (1753.75–2044.03) | 7.29 (6.74–7.41) | 1.891 | 1878.046 | 9.553 | 1880.308 | swift-frontend-stage | ✅ | ✅ |
| `aurorality/examples/hyperchat` | 10739.77 (10485.34–10739.79) | 424.45 (408.78–441.66) | 1776.65 (1659.63–1871.78) | 6.75 (6.51–12.60) | 4.186 | 0.000 | 1776.648 | 1769.903 | driver-overhead | ✅ | ❌ |
