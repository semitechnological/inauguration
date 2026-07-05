# Docs-site

Public site: **`https://inauguration.tsc.hk`** (source in **`docs-site/`**).

| Piece | Tool |
|-------|------|
| Landing WASM | `crepus web build` / `crepus web dev` |
| Markdown HTML | **`[targets.docs]`** in `docs-site/crepus.toml` → `scripts/docs-hook.sh` → `docs-gen` |
| Full dist | `in execute docs-site/build.in` (or `backend.in` → same) or `./scripts/build-docs-site.sh` |
| Splash copy | `docs-site/scripts/gen-splash.in` (run before `crepus web build`; `build-docs-site.sh` calls it) |

`crepus web dev` needs **`[targets.docs]`** so `/docs/` is populated (not “No generated docs output yet”).

## Commands

```bash
# from repo root
in execute docs-site/backend.in
crepus web dev --site docs-site
```

Footer: **built with crepuscularity + inauguration**.

## Deploy (Cloudflare)

```bash
./scripts/build-docs-site.sh
# Cloudflare Pages: upload docs-site/dist (dashboard or wrangler pages deploy docs-site/dist)
```

Custom domain **inauguration.tsc.hk** in Cloudflare DNS / Pages settings.