# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.
Wall times: **median** over `3` timed runs; **min–max** across those runs shown in parentheses next to medians (easy tables) or inline (detail table).

## Benchmark Environment

- Generated (UTC): `2026-05-08T17:08:24Z`
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

| Example | swift build median (min–max ms) | in median (min–max ms) |
|---|---:|---:|
| `aurorality/examples/counter` | 877.09 (807.85–904.97) | 7.26 (7.20–7.54) |
| `aurorality/examples/basic` | 871.34 (869.49–887.34) | 7.52 (6.47–7.90) |
| `aurorality/examples/hyperchat` | 355.74 (350.24–355.87) | 7.39 (6.65–7.52) |

## Swift Toolchain (Compiler Sources) Benchmark

| Example | swift build median (min–max ms) | in median (min–max ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 327.44 (324.32–335.10) | 6.56 (6.56–6.75) |

| Example | swiftc med (min–max) | swift build med (min–max) | in med (min–max) | hybrid-cli med (min–max) | in/swift-build | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `aurorality/examples/counter` | 171.31 (167.60–178.28) | 877.09 (807.85–904.97) | 7.26 (7.20–7.54) | 5.19 (5.18–6.08) | 0.008 | 0.044 | 7.212 | 2.062 | win | ✅ | ✅ |
| `aurorality/examples/basic` | 163.66 (158.35–174.27) | 871.34 (869.49–887.34) | 7.52 (6.47–7.90) | 6.24 (4.67–6.35) | 0.009 | 0.044 | 7.472 | 1.279 | win | ✅ | ✅ |
| `aurorality/examples/hyperchat` | 9571.92 (9372.87–9650.51) | 355.74 (350.24–355.87) | 7.39 (6.65–7.52) | 5.19 (5.16–5.41) | 0.021 | 0.042 | 7.351 | 2.204 | win | ✅ | ✅ |
