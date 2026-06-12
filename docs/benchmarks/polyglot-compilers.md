# Polyglot Compiler Matrix Benchmark

Measured against installed native compiler checks and `in compile --target native --entry answer --json` for the same polyglot sample files. The native mode column distinguishes object compilation, bytecode compilation, typechecking, and syntax-only checks; ratios are only directly comparable within the same mode. Missing native compilers are skipped; native compiler failures fail the script, while `in` failures are reported in the row.
Wall times: median over `3` timed runs; min-max across those runs shown in parentheses.

## Benchmark Environment

- Generated (UTC): `2026-06-12T05:40:33Z`
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
| C | `clang` | object | 53.04 (51.80-58.74) | 9.97 (9.32-11.96) | 0.188 | ok |  |
| C++ | `clang++` | object | 50.77 (50.45-51.44) | 9.39 (9.38-9.63) | 0.185 | ok |  |
| Rust | `rustc` | object | 37.83 (37.35-38.06) | 9.48 (9.45-9.75) | 0.251 | ok |  |
| Go | `go` | object | 16.46 (16.36-16.74) | 8.79 (8.46-8.98) | 0.534 | ok |  |
| Swift | `swiftc` | object | 122.70 (121.66-123.45) | 9.49 (9.00-9.58) | 0.077 | ok |  |
| V | `v` | syntax | 17.73 (17.61-17.97) | 8.68 (8.53-8.89) | 0.489 | ok |  |
| JavaScript | `node` | syntax | 29.04 (29.03-29.90) | 9.49 (9.43-9.58) | n/a | in failed | return value in void function `Counter_inc` |
| TypeScript | `bun` | typecheck | 560.64 (533.99-594.57) | 72.15 (19.08-91.35) | n/a | in failed | return type mismatch in `answer`: expected number, got Int |
| Python | `python3` | bytecode | 33.85 (33.67-35.89) | 10.25 (9.98-12.13) | n/a | in failed | duplicate parameter name `self` in `Counter___init__` |
| Ruby | `ruby` | syntax | 37.74 (36.58-38.65) | 9.99 (9.57-10.44) | n/a | in failed | return value in void function `answer` |
| Zig | `zig` | syntax | 58.79 (58.73-59.09) | 10.12 (9.70-10.90) | n/a | in failed | return type mismatch in `answer`: expected i32, got Int |
| PHP | `php` | syntax | 55.12 (54.08-56.40) | 10.06 (9.99-10.27) | n/a | in failed | unresolved assignment `v` in `Counter_inc` |
| Java | `javac` | bytecode | 196.07 (195.17-197.53) | 9.48 (9.44-9.53) | n/a | in failed | return type mismatch in `Sample_answer`: expected int, got Int |
| Nim | `nim` | typecheck | 179.54 (131.96-184.83) | 12.13 (9.37-14.96) | 0.068 | ok |  |
| D | `ldc2` | object | 41.99 (41.20-42.57) | 10.03 (9.61-10.22) | 0.239 | ok |  |

