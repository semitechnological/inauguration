# inauguration docs-site

**https://inauguration.tsc.hk** — crepuscularity web target (`crepus.toml` + `index.crepus`).

## Docs hook (`[targets.docs]`)

Flat Markdown at repo **`docs/`** (same layout as crepuscularity `docs/`; `internal/` is not published). Hook:

```toml
[targets.docs]
command = "bash"
args = ["scripts/docs-hook.sh"]
src = "../docs"
```

Required for **`crepus web dev`** `/docs/` routes.

## Quick start (CLI)

```bash
cargo install inauguration
in .                                    # build from checkout
in eval "print('hello world')"          # .in + ~40 languages, crates & libs
in languages --json
```

## Build site

Requires **crepuscularity-cli ≥ 0.9.18** (void `<br>` SSR) or `CREPU_ROOT` pointing at a matching checkout. `build-docs-site.sh` falls back to `CREPU_ROOT` when `crepus` on PATH is older.

```bash
cargo install crepuscularity-cli --version 0.9.18 --locked   # if needed
./scripts/build-docs-site.sh
# or: in execute docs-site/backend.in
```

## Deploy (Cloudflare Pages)

```bash
./scripts/deploy-docs-cloudflare.sh
# or: cd docs-site && wrangler pages deploy dist --project-name inauguration
```

Production URL: `https://inauguration-dwr.pages.dev` (until custom domain is attached).

**Custom domain `inauguration.tsc.hk`:** Cloudflare dashboard → Workers & Pages → **inauguration** → Custom domains → add `inauguration.tsc.hk` (zone already on your account, same as `alpenglow.tsc.hk`).

## Dev

```bash
crepus web dev --site "$(pwd)"
```