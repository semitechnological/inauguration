# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.
Each metric uses median of `1` runs.

## Benchmark Environment

- Generated (UTC): `2026-05-08T13:46:30Z`
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
- in binary: `/Users/undivisible/projects/inauguration/in-cli/target/debug/in`
- hybrid-cli binary: `/Users/undivisible/projects/inauguration/compiler/rust-driver/target/debug/hybrid-cli`

| Example | swiftc(ms) | swift build(ms) | in(ms) | hybrid-cli(ms) | in/swift-build | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `/Users/undivisible/projects/inauguration/../aurorality/examples/counter/Sources/App.swift` | 184.65 | 1112.77 | 273.30 | 7.33 | 0.246 | 0.016 | 273.282 | 265.965 | win | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/basic/Sources/App.swift` | 184.20 | 964.87 | 278.76 | 8.44 | 0.289 | 0.013 | 278.742 | 270.314 | win | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/hyperchat/Sources/HyperChatRootView.swift` | 11706.98 | 451.48 | 365.42 | 6.89 | 0.809 | 0.010 | 365.410 | 358.535 | win | ✅ | ✅ |
