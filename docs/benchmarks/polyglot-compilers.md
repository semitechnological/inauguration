# Polyglot Compiler Matrix Benchmark

Measured against installed native compiler checks and `in compile --target native --entry answer --json` for the same polyglot sample files. The native mode column distinguishes object compilation, bytecode compilation, typechecking, and syntax-only checks; ratios are only directly comparable within the same mode. Missing native compilers are skipped; native compiler failures fail the script, while `in` failures are reported in the row.
Wall times: median over `3` timed runs; min-max across those runs shown in parentheses.

## Benchmark Environment

- Generated (UTC): `2026-06-12T07:41:57Z`
- Host OS: `Darwin`
- Kernel: `27.0.0`
- CPU: `Apple M5 Pro`
- Memory: `51539607552`
- BENCH_RUNS: `3`
- BENCH_WARMUP_RUNS: `1`
- in binary: `/Users/undivisible/projects/inauguration/in-cli/target/debug/in`

## Results

| Language | Native compiler | Native mode | Native median (min-max ms) | in median (min-max ms) | in/native | Status | Reason |
|---|---|---|---:|---:|---:|---|---|
| C | `clang` | object | 46.19 (44.55-61.29) | 7.81 (7.74-8.71) | 0.169 | ok |  |
| C++ | `clang++` | object | 45.23 (45.08-50.08) | 8.23 (8.05-8.38) | 0.182 | ok |  |
| Rust | `rustc` | object | 33.31 (33.11-33.74) | 8.14 (7.87-8.36) | 0.244 | ok |  |
| Go | `go` | object | 15.00 (14.41-16.33) | 7.68 (7.56-8.18) | 0.512 | ok |  |
| Swift | `swiftc` | object | 112.21 (111.98-112.94) | 8.53 (8.18-8.56) | 0.076 | ok |  |
| V | `v` | syntax | 16.20 (16.18-16.32) | 7.77 (7.74-7.91) | 0.479 | ok |  |
| JavaScript | `node` | syntax | 27.17 (26.87-28.42) | 8.21 (8.00-8.24) | 0.302 | ok |  |
| TypeScript | `bun` | typecheck | 409.34 (407.08-412.88) | 8.92 (8.92-9.28) | 0.022 | ok |  |
| Python | `python3` | bytecode | 27.50 (27.33-27.58) | 8.32 (8.13-8.37) | 0.302 | ok |  |
| Ruby | `ruby` | syntax | 32.60 (32.33-32.71) | 7.73 (7.56-7.82) | 0.237 | ok |  |
| Zig | `zig` | syntax | 52.85 (52.85-53.46) | 7.83 (7.78-8.05) | 0.148 | ok |  |
| PHP | `php` | syntax | 48.40 (47.74-48.76) | 8.13 (7.94-8.20) | 0.168 | ok |  |
| Java | `javac` | bytecode | 170.56 (169.64-171.50) | 8.92 (8.85-8.92) | 0.052 | ok |  |
| Nim | `nim` | typecheck | 105.51 (105.51-106.40) | 8.67 (8.51-8.77) | 0.082 | ok |  |
| D | `ldc2` | object | 34.92 (34.67-35.16) | 8.07 (8.02-8.07) | 0.231 | ok |  |

