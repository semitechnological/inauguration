# in CLI

Thin command surface for inauguration MVP.

## Install

```bash
cargo install --path . --bin in --force
```

## Commands

- `in build` -> runs `hybrid-cli` from `compiler/rust-driver`
- `in dev` -> runs `scripts/dev-loop.sh`
- `in run` -> runs hotreload daemon only
- `in test` -> runs Rust + OCaml + Swift + daemon test lanes
- `in doctor` -> checks `cargo`, `swift`, `opam`, `dune`
