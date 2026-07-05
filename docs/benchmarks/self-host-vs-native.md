# Self-host vs native benchmarks

**Regenerate:** `./scripts/bench-self-host.sh` → [`self-host-vs-native.json`](self-host-vs-native.json)

Live numbers below are filled from the JSON when you run the script. Stale static tables were removed so the site does not show v0.7.1 fiction.

## What we measure

| Stage | Command | Meaning |
|-------|---------|---------|
| Self-host front + JIT | `in build --path in-cli/src/main.rs --verbose` | Rust `main.rs` parsed/typed through owned Core IR; JIT lowering when it succeeds |
| Native artifact | `in build --path in-cli/src/main.rs --out /tmp/in` | Same front; Mach-O link when stdlib/native surface allows |
| Call graph | `in compile --path … --target jit --json` | `call_edge_count` from SIL (when compile succeeds) |

## Latest run (from JSON)

<!-- BENCH:SELF_HOST_START -->
| Field | Value |
|-------|------:|
| Generated (UTC) | 2026-07-05T13:32:04Z |
| `in` version | in 0.7.4 |
| Host / CPU | Darwin / Apple M5 Pro |
| Functions parsed | 1,879 |
| Functions typed | 1,879 |
| Call edges | 4,268 |
| Wall ms (avg / runs) | 156.8 / [156.822] |
| JIT compile µs | 35,753 |
| Front+JIT OK | True |
| Native `--out` | blocked — in build: native compilation failed (native-lowering-failed) |
<!-- BENCH:SELF_HOST_END -->

## Language coverage

| Language | Compile | Execute | Notes |
|----------|:-------:|:-------:|-------|
| `.in` | ✅ | ✅ | language subset, JIT |
| `.icore` | ✅ | ✅ | Core IR JSON, JIT |
| `.rs` (samples) | ✅ | ✅ | simple fns → native Mach-O |
| `.rs` (in-cli `main.rs`) | ✅ | ⚠️ | parse/type; JIT/native blocked on stdlib types (e.g. atomics) |
| `.zig` / `.go` / `.swift` | ✅ | ✅ | polyglot samples + gates |
| `.poly` | ✅ | ✅ | eval harness |

See [polyglot-compilers.md](polyglot-compilers.md) for cross-compiler timings.

## Next optimizations

1. **Daemon** — `in daemon start` for repeated self-host checks (cold binary ~tens of ms).
2. **Family typecheck** — matrix `can_typecheck()` now drives `uses_family_typecheck` (Go, Swift, C, … gates align with docs).
3. **Native self-host** — C-ABI / `Named` type coverage for remaining Rust stdlib shapes in `native_emit`.
4. **Bench CI** — optional workflow step to commit JSON on release tags.