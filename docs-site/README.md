# inauguration docs-site

Dark, information-dense hub for **inlang** (`.in`) and the **inauguration** compiler. Same layout as [crepuscularity/docs-site](../crepuscularity/docs-site): **`crepus.toml`**, `index.crepus`, `site.json`, `runtime/`, `static/`.

Theme: zinc terminal (Chivo Mono), tsc.hk tone; doc grid density like crepuscularity’s landing.

Footer: **built with crepuscularity + inauguration**.

## Prerequisites

- Sibling **`../crepuscularity`** (`docs-site/runtime/Cargo.toml` path dep).
- **`crepus`** on `PATH`, or `CREPU_ROOT` + `scripts/build-docs-site.sh` fallback.
- **wasm32** + matching **wasm-bindgen-cli** (see crepuscularity `docs/cli.md`).

## Markdown sources

`crepus web build` emits **top-level** `docs/*.md` only. Canonical files live under `docs/architecture/` and `docs/benchmarks/`; **symlinks** at `docs/*.md` point there.

Post-build: **`scripts/patch-docs-site-instrument-sans.sh`** themes generated HTML pages.

## Build

```bash
in execute docs-site/backend.in    # inlang: crepus web build + theme patch
./scripts/build-docs-site.sh       # bash equivalent
```

Output: **`docs-site/dist/`** (gitignored).

## Develop

```bash
crepus web serve --site /path/to/inauguration/docs-site
```

Or from crepuscularity checkout:

```bash
cargo run -p crepuscularity-cli --manifest-path ../crepuscularity/Cargo.toml -- \
  web serve --site "$(pwd)/docs-site"
```

Serve over HTTP (WASM): e.g. `cd docs-site/dist && python3 -m http.server 8765`.