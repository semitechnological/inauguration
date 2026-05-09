# inauguration docs site (crepuscularity-web)

Twilight-styled static site matching [crepuscularity `docs-site`](https://github.com/semitechnological/crepuscularity/tree/main/docs-site): `.crepus` landing page, wasm32 runtime, and HTML emitted from repository `docs/*.md`.

## Prerequisites

- Sibling clone: **`../crepuscularity`** (path dependency in `runtime/Cargo.toml`).
- [`crepus`](https://github.com/semitechnological/crepuscularity) CLI on `PATH`, **or** use the build script which falls back to `cargo run` in that repo.
- **wasm32** target and **wasm-bindgen-cli** whose schema matches the workspace `wasm-bindgen` crate (if the build fails at the wasm-bindgen step, run `cargo install -f wasm-bindgen-cli --version 0.2.121` or the version your `Cargo.lock` uses). See crepuscularity `docs/cli.md`.

## Markdown layout

`crepus web build` only picks up **top-level** `docs/*.md`. This repo keeps canonical sources under `docs/architecture/` and `docs/benchmarks/`; **symlinks** at `docs/*.md` point at those files so the doc emitter finds them.

## Build

```bash
./scripts/build-docs-site.sh
```

Output: **`docs-site/dist/`** (gitignored). Serve over HTTP (WASM modules):

```bash
cd docs-site/dist && python3 -m http.server 8765
```

## Develop

```bash
cd /path/to/crepuscularity
cargo run -p crepuscularity-cli -- web serve --site /path/to/inauguration/docs-site
```
