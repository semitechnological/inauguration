# in CLI

Thin command surface for inauguration MVP.

## Install

```bash
cargo install --path . --bin in --force
```

## Commands

- `in build` -> runs `hybrid-cli` from `compiler/rust-driver`
- `in build --path <dir>` -> auto-batch mode (default behavior for directories)
- `in dev` -> runs `scripts/dev-loop.sh`
- `in run` -> runs hotreload daemon only
- `in test` -> runs Rust + OCaml + Swift + daemon test lanes
- `in doctor` -> checks `cargo`, `swift`, `opam`, `dune`

## Plugins

```bash
in plugin list
in plugin install aurorality
in plugin install crepuscularity
in plugin run aurorality --target ../aurorality
```
