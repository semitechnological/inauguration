# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.
Each metric uses median of `3` runs.

## Benchmark Environment

- Generated (UTC): `2026-05-08T16:53:07Z`
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

| Example | swift build(ms) | in(ms) |
|---|---:|---:|
| `aurorality/examples/counter` | 852.58 | 8.11 |
| `aurorality/examples/basic` | 958.66 | 7.29 |
| `aurorality/examples/hyperchat` | 365.90 | 7.76 |

## Swift Toolchain (Compiler Sources) Benchmark

| Example | swift build(ms) | in(ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 354.11 | 7.41 |

| Example | swiftc(ms) | swift build(ms) | in(ms) | hybrid-cli(ms) | in/swift-build | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `aurorality/examples/counter` | 157.19 | 852.58 | 8.11 | 6.06 | 0.010 | 0.048 | 8.061 | 2.051 | win | ✅ | ✅ |
| `aurorality/examples/basic` | 218.98 | 958.66 | 7.29 | 5.66 | 0.008 | 0.046 | 7.246 | 1.632 | win | ✅ | ✅ |
| `aurorality/examples/hyperchat` | 10304.84 | 365.90 | 7.76 | 6.05 | 0.021 | 0.044 | 7.717 | 1.716 | win | ✅ | ✅ |
