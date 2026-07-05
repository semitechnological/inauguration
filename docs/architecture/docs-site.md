# Docs-site

Public site: **`https://inauguration.tsc.hk`** (source in **`docs-site/`**).

| Piece | Tool |
|-------|------|
| Landing WASM | `crepus web build` / `crepus web dev` |
| Markdown HTML | **`[targets.docs]`** in `docs-site/crepus.toml` → `scripts/docs-hook.sh` → `docs-gen` |
| Full dist | `in execute docs-site/backend.in` or `./scripts/build-docs-site.sh` |

`crepus web dev` needs **`[targets.docs]`** so `/docs/` is populated (not “No generated docs output yet”).

## Commands

```bash
# from repo root
in execute docs-site/backend.in
crepus web dev --site docs-site
```

Footer: **built with crepuscularity + inauguration**.