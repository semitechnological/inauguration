# Self-host vs native benchmarks

**Regenerate:** `./scripts/bench-self-host.sh && python3 scripts/render-self-host-bench-md.py`

Compares **inauguration’s owned Rust front** (`in build` on `in-cli/src/main.rs`) to **rustc/Cargo** building the same CLI crate: compile wall time, binary size, and cold process startup.

## What we measure

| Stage | Command | Meaning |
|-------|---------|---------|
| Owned front + JIT | `in build --path in-cli/src/main.rs` | Parse/type ~1.9k fns; JIT lowering (may fail on stdlib) |
| rustc release | `cargo build --release -p inauguration` | Full crate + deps link to `target/release/in` |
| Native self-artifact | `in build --path … --out /tmp/in` | Target: Mach-O from owned pipeline (blocked today) |
| Stats | `in compile --path … --target jit --json` | `parsed_function_count`, `call_edge_count` |

## Latest: `in` self-host

<!-- BENCH:SELF_HOST_START -->
| Field | Value |
|-------|------:|
| Generated (UTC) | 2026-07-05T13:43:04Z |
| `in` version | in 0.7.4 |
| Host / CPU | Darwin / Apple M5 Pro |
| Functions parsed | 1,879 |
| Functions typed | 1,879 |
| Call edges | 4,268 |
| `in build` wall ms (avg) | 197.0 |
| JIT lowering µs (last run) | 36,667 |
| Front stats OK | True |
| Native `--out` | blocked — in build: native compilation failed (native-lowering-failed) |
<!-- BENCH:SELF_HOST_END -->

## vs rustc (same shipped binary today)

<!-- BENCH:RUSTC_CMP_START -->
| Metric | `in` (owned front) | `rustc` / Cargo (release) |
|--------|-------------------:|-------------------------:|
| Compile wall (avg ms) | 197.0 | 49950.0 |
| Speed ratio (in ÷ rustc) | 0.004× | 1.000× (baseline) |
| Shipped binary size | 69.17 MiB (72,531,952 B) | 69.17 MiB (72,531,952 B) |
| Size ratio (in ÷ rustc) | 1.000× | same artifact today |
| Cold `in --version` (ms) | 157.35 | 7.33 |

**Notes:** `in build` parses/types **main.rs** through Core IR (JIT often fails on atomics); **Cargo** rebuilds the **whole** `inauguration` crate after `touch main.rs`. Binary row is the **same** `target/release/in` until native self-link works. Execution row is **CLI startup**, not compile throughput.
<!-- BENCH:RUSTC_CMP_END -->

## Language coverage

| Language | Compile | Execute | Notes |
|----------|:-------:|:-------:|-------|
| `.in` | ✅ | ✅ | JIT |
| `.rs` (in-cli `main.rs`) | ✅ | ⚠️ | front OK; JIT/native blocked on `AtomicPtr` etc. |
| `.rs` (samples) | ✅ | ✅ | native Mach-O |

[Polyglot compile times](polyglot-compilers.md).

## Optimization roadmap

| Priority | Item | Status |
|----------|------|--------|
| P0 | **Named/stdlib types** in `native_emit` (`AtomicPtr`, atomics) | blocked native + full JIT self-build |
| P1 | **Daemon** `in daemon start` — drop cold startup on repeated benches | doc’d |
| P1 | **Compile cache** — persist lowered native / skip re-link `inrt` | partial (source hash) |
| P2 | **Incremental Rust front** — re-parse from Core IR cache | parallel parse landed |
| P2 | **Bench on release tag** — script + JSON in repo; optional deploy refresh | script ready |
| P3 | **Fair rustc compare** — `cargo clean` cold + `in` full native artifact size | future |

**Done recently:** family typecheck ↔ `language_support`; live JSON bench; `.in` strict `name: Type` params.