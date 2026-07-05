# Polyglot Compiler Matrix Benchmark

Measured against installed native compiler checks and `in compile --target native --entry answer --json` for the same polyglot sample files. The native mode column distinguishes object compilation, bytecode compilation, typechecking, and syntax-only checks; ratios are only directly comparable within the same mode. Missing native compilers are skipped; native compiler failures fail the script, while `in` failures are reported in the row.
Wall times: median over `3` timed runs; min-max across those runs shown in parentheses.

## Benchmark Environment

- Generated (UTC): `2026-07-05T03:45:06Z`
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
| C | `clang` | object | 68.66 (61.93-73.29) | 15.91 (15.82-15.94) | 0.232 | ok |  |
| C++ | `clang++` | object | 62.49 (58.77-64.44) | 15.90 (14.58-15.91) | 0.254 | ok |  |
| Rust | `rustc` | object | 56.04 (54.34-59.05) | 15.76 (14.88-16.36) | 0.281 | ok |  |
| Go | `go` | object | 24.25 (23.64-28.75) | 13.84 (13.83-13.91) | 0.571 | ok |  |
| Swift | `swiftc` | object | 149.88 (146.75-153.73) | 14.78 (13.52-14.82) | 0.099 | ok |  |
| V | `v` | syntax | 25.29 (24.43-25.67) | 13.50 (12.52-13.73) | 0.534 | ok |  |
| JavaScript | `node` | syntax | 35.41 (33.39-36.68) | 12.92 (12.41-13.74) | 0.365 | ok |  |
| TypeScript | `bun` | typecheck | 1118.70 (1049.15-1224.45) | 13.41 (12.68-14.57) | 0.012 | ok |  |
| Python | `python3` | bytecode | 36.94 (34.93-38.12) | 12.75 (12.58-13.11) | 0.345 | ok |  |
| Ruby | `ruby` | syntax | 43.10 (43.08-54.99) | 12.66 (12.25-13.09) | 0.294 | ok |  |
| Zig | `zig` | syntax | 59.38 (58.09-60.08) | 12.49 (12.47-14.46) | 0.210 | ok |  |
| PHP | `php` | syntax | 62.34 (60.09-67.66) | 13.90 (13.12-16.18) | 0.223 | ok |  |
| Java | `javac` | bytecode | 189.66 (181.57-193.15) | 12.29 (12.21-13.33) | 0.065 | ok |  |
| Nim | `nim` | typecheck | 131.96 (123.77-140.85) | 13.55 (12.36-16.62) | 0.103 | ok |  |
| D | `ldc2` | object | 42.56 (41.66-44.01) | 13.45 (12.47-13.69) | 0.316 | ok |  |

