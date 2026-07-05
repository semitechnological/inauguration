# inauguration docs-site

**https://inauguration.tsc.hk** — crepuscularity web target (`crepus.toml` + `index.crepus`).

## Docs hook (`[targets.docs]`)

Markdown at repo **`docs/`** (symlinks into `architecture/` + `benchmarks/`). Hook:

```toml
[targets.docs]
command = "bash"
args = ["scripts/docs-hook.sh"]
src = "../docs"
```

Required for **`crepus web dev`** `/docs/` routes.

## Build

```bash
in execute docs-site/backend.in
./scripts/build-docs-site.sh
```

## Deploy (Cloudflare)

```bash
./scripts/build-docs-site.sh
wrangler pages deploy dist --project-name inauguration   # or upload dist/ in dashboard
```

## Dev

```bash
crepus web dev --site "$(pwd)"
```