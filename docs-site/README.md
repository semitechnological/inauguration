# inauguration docs-site

**https://inauguration.tsc.hk** — landing + docs, served by a Bun + Moonshine
runtime using `@tschk/crepus-moonshine` (Crepus IR → React renderer).

The legacy crepuscularity-web target (`crepus.toml` + `index.crepus` + `runtime/`)
is kept alongside for reference; the live server lives in `src/`.

## Stack

- `src/ir.ts` — page content as `CrepusIr` (inline styles, dark zinc + JetBrains Mono).
- `src/head.ts` — `<head>` HTML: SEO meta, fonts, and the CSS (animations) ported from `crepus.toml` `head_html`.
- `src/server.ts` — `createBunServer` + `crepusRenderer` (head-injecting wrapper); serves `/`, `static/`, and generated `dist/docs/`.
- `src/build.ts` — prerenders the page to `dist/index.html`.

## Benchmarks

Localhost, warm cache, 10 sequential `GET /` requests (macOS arm64, Bun 1.3.14).
The legacy stack ships an 8 KB HTML shell that hydrates client-side via WASM; the
moonshine stack returns fully server-rendered HTML, yet is faster per request.

| Metric | Before (crepuscularity-web) | After (moonshine) |
|--------|----------------------------|-------------------|
| Avg response time | 7.7ms | 1.4ms |
| TTFB | 5.4ms | 0.6ms |
| HTML size | 7.9KB | 25.1KB |
| Stack | Rust WASM + UnoCSS | Bun + React + Crepus IR |

Reproduce:

```bash
# before: crepus web build --site . && python3 -m http.server 3011 -d dist
# after:  PORT=3012 bun run start
curl -s -o /dev/null -w "%{time_total}\n" http://localhost:3011/
```

## Dev

```bash
bun install
bun run dev          # http://localhost:3000 (hot reload)
bun run build        # prerender dist/index.html
bun test             # starts server, fetches /, asserts 200 + HTML
bun run typecheck    # tsc --noEmit
```

Port: `PORT=4000 bun run dev`.

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