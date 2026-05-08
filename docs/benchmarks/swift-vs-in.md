# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.
Each metric uses median of `1` runs.

## Benchmark Environment

- Generated (UTC): `2026-05-08T13:50:33Z`
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
| `/Users/undivisible/projects/inauguration/../aurorality/examples/counter/Sources/App.swift` | 289.04 | 1062.41 | 839.31 | 9.87 | 0.790 | 0.020 | 839.294 | 829.440 | win | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/basic/Sources/App.swift` | 172.78 | 874.69 | 8.04 | 6.10 | 0.009 | 0.016 | 8.019 | 1.933 | win | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/hyperchat/Sources/HyperChatRootView.swift` | 9147.85 | 351.10 | 8.15 | 5.52 | 0.023 | 0.015 | 8.139 | 2.637 | win | ✅ | ✅ |
