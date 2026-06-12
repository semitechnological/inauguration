# Polyglot Compiler Matrix Benchmark

Measured against installed native compiler checks and `in compile --target native --entry answer --json` for the same polyglot sample files. The native mode column distinguishes object compilation, bytecode compilation, typechecking, and syntax-only checks; ratios are only directly comparable within the same mode. Missing native compilers are skipped; installed compiler failures are recorded and fail the script.
Wall times: median over `3` timed runs; min-max across those runs shown in parentheses.

## Benchmark Environment

- Generated (UTC): `2026-06-12T05:33:35Z`
- Host OS: `Darwin`
- Kernel: `27.0.0`
- CPU: `Apple M5 Pro`
- Memory: `51539607552`
- BENCH_RUNS: `3`
- BENCH_WARMUP_RUNS: `1`
- in binary: `/Users/undivisible/projects/inauguration/in-cli/target/debug/in`

## Results

| Language | Native compiler | Native mode | Native median (min-max ms) | in median (min-max ms) | in/native | Status |
|---|---|---|---:|---:|---:|---|
| C | `clang` | object | 64.74 (61.42-81.89) | 11.40 (10.62-19.96) | 0.176 | ok |
| C++ | `clang++` | object | 58.08 (55.70-58.44) | 10.27 (9.78-10.90) | 0.177 | ok |
| Rust | `rustc` | object | 38.68 (38.34-42.71) | 9.81 (9.23-11.37) | 0.254 | ok |
| Go | `go` | object | 40.73 (23.60-40.91) | 13.23 (12.05-137.70) | 0.325 | ok |
| Swift | `swiftc` | object | 151.22 (147.31-173.78) | 12.24 (11.23-12.96) | 0.081 | ok |
| V | `v` | syntax | 24.59 (23.27-134.92) | 10.95 (10.58-19.07) | 0.445 | ok |
| JavaScript | `node` | syntax | 39.59 (35.16-64.15) | 14.04 (13.04-16.19) | 0.355 | ok |
| TypeScript | `bun` | typecheck | 689.77 (570.59-761.71) | 12.11 (11.91-13.43) | 0.018 | ok |
| Python | `python3` | bytecode | 37.95 (35.41-57.27) | 10.74 (10.60-12.62) | 0.283 | ok |
| Ruby | `ruby` | syntax | 42.62 (41.09-43.17) | 11.00 (10.48-11.81) | 0.258 | ok |
| Zig | `zig` | syntax | 64.39 (61.39-70.21) | 10.89 (10.04-11.44) | 0.169 | ok |
| PHP | `php` | syntax | 56.25 (53.78-57.61) | 10.71 (10.44-11.27) | 0.190 | ok |
| Java | `javac` | bytecode | 258.97 (208.07-292.80) | 11.25 (10.80-14.54) | 0.043 | ok |
| Nim | `nim` | typecheck | 147.62 (145.32-156.99) | 11.23 (11.06-12.33) | 0.076 | ok |
| D | `ldc2` | object | 40.41 (40.26-41.37) | 9.74 (9.70-10.03) | 0.241 | ok |

