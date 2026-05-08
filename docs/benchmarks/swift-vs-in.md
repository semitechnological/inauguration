# Swift Compiler vs in Pipeline Benchmark

Measured with: raw `swiftc -typecheck`, package-context `swift build`, and `in build`.
Each metric uses median of `1` runs.

## Benchmark Environment

- Generated (UTC): `2026-05-08T13:57:26Z`
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

| Example | swift build(ms) | in(ms) | in faster |
|---|---:|---:|:---:|
| `/Users/undivisible/projects/inauguration/../aurorality/examples/counter/Sources/App.swift` | 1097.83 | 9.17 | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/basic/Sources/App.swift` | 939.48 | 7.63 | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/hyperchat/Sources/HyperChatRootView.swift` | 321.51 | 6.82 | ✅ |

| Example | swiftc(ms) | swift build(ms) | in(ms) | hybrid-cli(ms) | in/swift-build | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `/Users/undivisible/projects/inauguration/../aurorality/examples/counter/Sources/App.swift` | 161.85 | 1097.83 | 9.17 | 7.24 | 0.008 | 0.016 | 9.158 | 1.938 | win | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/basic/Sources/App.swift` | 157.76 | 939.48 | 7.63 | 6.41 | 0.008 | 0.016 | 7.613 | 1.224 | win | ✅ | ✅ |
| `/Users/undivisible/projects/inauguration/../aurorality/examples/hyperchat/Sources/HyperChatRootView.swift` | 9169.57 | 321.51 | 6.82 | 4.81 | 0.021 | 0.015 | 6.804 | 2.005 | win | ✅ | ✅ |
