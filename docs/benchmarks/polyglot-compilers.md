# Polyglot Compiler Matrix Benchmark

Measured against native compiler frontend checks for installed tools and `in compile --target native --entry answer --json` for the same polyglot sample files. Missing native compilers are skipped; installed compiler failures are recorded and fail the script.
Wall times: median over `3` timed runs; min-max across those runs shown in parentheses.

## Benchmark Environment

- Generated (UTC): `2026-06-12T05:22:51Z`
- Host OS: `Darwin`
- Kernel: `27.0.0`
- CPU: `Apple M5 Pro`
- Memory: `51539607552`
- BENCH_RUNS: `3`
- BENCH_WARMUP_RUNS: `1`
- in binary: `/Users/undivisible/projects/inauguration/in-cli/target/debug/in`

## Results

| Language | Native compiler | Native median (min-max ms) | in median (min-max ms) | in/native | Status |
|---|---|---:|---:|---:|---|
| C | `clang` | 61.56 (54.10-71.40) | 12.84 (11.95-14.35) | 0.209 | ok |
| C++ | `clang++` | 58.09 (56.94-58.97) | 10.94 (10.84-14.11) | 0.188 | ok |
| Rust | `rustc` | 37.19 (37.11-37.49) | 11.00 (10.29-11.20) | 0.296 | ok |
| Go | `go` | 18.64 (18.49-19.06) | 10.11 (9.74-11.08) | 0.542 | ok |
| Swift | `swiftc` | 127.95 (127.51-129.43) | 10.17 (9.86-10.40) | 0.079 | ok |
| V | `v` | 19.10 (18.65-19.77) | 10.12 (9.73-10.25) | 0.530 | ok |
| JavaScript | `node` | 32.08 (31.62-32.58) | 10.93 (10.51-11.13) | 0.341 | ok |
| TypeScript | `bun` | 519.15 (514.58-534.88) | 11.99 (11.61-12.50) | 0.023 | ok |
| Python | `python3` | 34.78 (34.35-48.67) | 12.42 (12.28-13.24) | 0.357 | ok |
| Ruby | `ruby` | 40.57 (39.01-41.65) | 11.20 (10.74-11.61) | 0.276 | ok |
| Zig | `zig` | 58.34 (57.95-58.48) | 10.71 (10.65-10.97) | 0.184 | ok |
| PHP | `php` | 54.51 (54.15-56.46) | 10.65 (10.46-11.31) | 0.195 | ok |
| Java | `javac` | 203.54 (198.91-301.69) | 10.83 (10.38-14.90) | 0.053 | ok |
| Nim | `nim` | 162.86 (138.21-188.02) | 12.11 (11.33-53.52) | 0.074 | ok |
| D | `ldc2` | 42.74 (41.44-44.32) | 10.38 (10.30-10.53) | 0.243 | ok |

