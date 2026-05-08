# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.
Each metric uses median of `3` runs.

## Benchmark Environment

- Generated (UTC): `2026-05-08T16:17:59Z`
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
| `aurorality/examples/counter` | 789.62 | 5.14 |
| `aurorality/examples/basic` | 763.95 | 4.85 |
| `aurorality/examples/hyperchat` | 320.13 | 5.12 |

## Swift Toolchain (Compiler Sources) Benchmark

| Example | swift build(ms) | in(ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 321.65 | 5.15 |

| Example | swiftc(ms) | swift build(ms) | in(ms) | hybrid-cli(ms) | in/swift-build | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `aurorality/examples/counter` | 150.72 | 789.62 | 5.14 | 4.70 | 0.007 | 0.019 | 5.117 | 0.437 | win | ✅ | ✅ |
| `aurorality/examples/basic` | 141.91 | 763.95 | 4.85 | 4.47 | 0.006 | 0.018 | 4.827 | 0.379 | win | ✅ | ✅ |
| `aurorality/examples/hyperchat` | 9139.58 | 320.13 | 5.12 | 4.72 | 0.016 | 0.018 | 5.103 | 0.397 | win | ✅ | ✅ |
