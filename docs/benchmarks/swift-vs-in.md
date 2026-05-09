# Swift Compiler vs in Pipeline Benchmark

Measured with: **`swiftc -typecheck`** on a single file when there is no local `Package.swift`; when there is a package, **`scripts/swiftc-bench-typecheck.sh`** (same Sources + Generated inputs and Clang flags idea as `in-cli` **`sil_emit`**) after a timed **`swift build`**. Also package-context **`swift build`** (SwiftPM reference) and **`in build`** default (**native hybrid pipeline only**, no SwiftPM).
**in** column = inauguration compile path (scheduler + SIL passes today); **swift build** = legacy SwiftPM baseline until native codegen fully replaces it.
**hybrid-cli** matches the native wave harness without the **`in`** CLI wrapper overhead.
Wall times: **median** over `2` timed runs; **min–max** across those runs shown in parentheses next to medians (easy tables) or inline (detail table).

## Benchmark Environment

- Generated (UTC): `2026-05-09T14:04:13Z`
- Host OS: `Darwin`
- Kernel: `25.4.0`
- CPU: `Apple M5 Pro`
- Memory: `51539607552`
- Swift: `swift-driver version: 1.148.6 Apple Swift version 6.3 (swiftlang-6.3.0.123.5 clang-2100.0.123.102)
Target: arm64-apple-macosx26.0`
- Rustc: `rustc 1.94.1 (e408947bf 2026-03-25)`
- Cargo: `cargo 1.94.1 (29ea6fb6a 2026-03-24)`
- V: `V 0.5.0 e2f5d6c`
- BENCH_RUNS: `2`
- BENCH_WARMUP_RUNS: `1`
- in binary: `/Users/undivisible/projects/inauguration/in-cli/target/debug/in`
- hybrid-cli binary: `/Users/undivisible/projects/inauguration/compiler/rust-driver/target/debug/hybrid-cli`

## Easy Copy/Paste

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `aurorality/examples/counter` | 915.18 (913.74–916.62) | 1661.45 (1580.50–1742.39) |
| `aurorality/examples/basic` | 963.68 (873.88–1053.48) | 1695.06 (1589.27–1800.85) |
| `aurorality/examples/hyperchat` | 359.42 (357.01–361.83) | 3615.55 (3587.73–3643.36) |

## Swift Toolchain (Compiler Sources) Benchmark

| Example | SwiftPM swift build median (min–max ms) | in native median (min–max ms) |
|---|---:|---:|
| `vendor/swift/SwiftCompilerSources` | 384.87 (384.25–385.49) | 609.08 (597.95–620.21) |

| Example | swiftc med (min–max) | SwiftPM med (min–max) | in native med (min–max) | hybrid-cli med (min–max) | native÷SwiftPM | in-stage-total(ms) | in-driver-overhead(ms) | in-wrapper-overhead(ms) | loss bucket | swift build ok | in ok |
|---|---:|---:|---:|---:|---:|---:|---:|---:|---|:---:|:---:|
| `aurorality/examples/counter` | 360.86 (351.88–369.84) | 915.18 (913.74–916.62) | 1661.45 (1580.50–1742.39) | 6.29 (5.72–6.86) | 1.815 | 1653.351 | 8.094 | 1655.155 | swift-frontend-stage | ✅ | ✅ |
| `aurorality/examples/basic` | 347.24 (346.46–348.01) | 963.68 (873.88–1053.48) | 1695.06 (1589.27–1800.85) | 11.55 (6.40–16.70) | 1.759 | 1686.904 | 8.157 | 1683.509 | swift-frontend-stage | ✅ | ✅ |
| `aurorality/examples/hyperchat` | 1782.96 (1713.85–1852.07) | 359.42 (357.01–361.83) | 3615.55 (3587.73–3643.36) | 5.86 (5.63–6.10) | 10.059 | 3606.743 | 8.804 | 3609.683 | swift-frontend-stage | ✅ | ✅ |
