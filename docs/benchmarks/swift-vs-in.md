# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.
Each metric uses median of `1` runs.

## Benchmark Environment

- Generated (UTC): `2026-05-08T15:52:26Z`
- Host OS: `Darwin`
- Kernel: `25.4.0`
- CPU: `Apple M5 Pro`
- Memory: `51539607552`
- Swift: `swift-driver version: 1.148.6 Apple Swift version 6.3 (swiftlang-6.3.0.123.5 clang-2100.0.123.102)
Target: arm64-apple-macosx26.0`
- Rustc: `rustc 1.94.1 (e408947bf 2026-03-25)`
- Cargo: `cargo 1.94.1 (29ea6fb6a 2026-03-24)`
- V: `V 0.5.0 e2f5d6c`
- BENCH_RUNS: `1`
- BENCH_WARMUP_RUNS: `1`
- in binary: `/Users/undivisible/projects/inauguration/in-cli/target/debug/in`
- hybrid-cli binary: `/Users/undivisible/projects/inauguration/compiler/rust-driver/target/debug/hybrid-cli`

## Easy Copy/Paste

| Example | swift build(ms) | in(ms) |
|---|---:|---:|
| `aurorality/examples/counter` | 824.24 | 7.49 |
| `aurorality/examples/basic` | 879.75 | 7.06 |
| `aurorality/examples/hyperchat` | 330.71 | 7.18 |

## Swift Toolchain (Compiler Sources) Benchmark

| Example | swift build(ms) | in(ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 327.42 | 12.48 |

| Example | swiftc(ms) | swift build(ms) | in(ms) | hybrid-cli(ms) | in/swift-build | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `aurorality/examples/counter` | 185.92 | 824.24 | 7.49 | 4.54 | 0.009 | 0.014 | 7.473 | 2.946 | win | ✅ | ✅ |
| `aurorality/examples/basic` | 170.78 | 879.75 | 7.06 | 4.89 | 0.008 | 0.016 | 7.042 | 2.165 | win | ✅ | ✅ |
| `aurorality/examples/hyperchat` | 9716.06 | 330.71 | 7.18 | 5.15 | 0.022 | 0.015 | 7.165 | 2.028 | win | ✅ | ✅ |
