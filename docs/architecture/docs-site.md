# Docs-site

Public docs live in **`docs-site/`**, same shape as **crepuscularity/docs-site** (`crepus.toml`, `index.crepus`, WASM runtime).

| Piece | Owner |
|-------|--------|
| `.crepus` landing, `crepus web build` / `web serve` | **Crepuscularity** |
| Markdown canon | **`docs/architecture/`**, **`docs/benchmarks/`** (symlinks at `docs/*.md`) |
| HTML theme patch | **`scripts/patch-docs-site-instrument-sans.sh`** |

**Inlang** content is markdown + landing copy; **inauguration** does not add docs-specific `in` subcommands—use **`crepus`** like any other crepuscularity web site.

## Build (inlang)

```bash
in execute docs-site/backend.in
```

## Serve

```bash
crepus web serve --site docs-site
```

Footer: **built with crepuscularity + inauguration**.

## See also

- `docs-site/README.md`
- [in-language.md](in-language.md)