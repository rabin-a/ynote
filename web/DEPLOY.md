# Deploying ynote.onl to Vercel

The web app is a **static site** — Vercel runs no build and hosts no data. It
serves `web/` (HTML/JS/CSS + two prebuilt WASM bundles). All rendering happens
in the browser; notes live in the visitor's `localStorage`. There is no backend.

## One-time setup

```bash
npm i -g vercel
vercel login
vercel link            # from the repo root; links this repo to a Vercel project
```

`vercel.json` (repo root) already sets `outputDirectory: "web"` and disables the
build/install steps. `.vercelignore` ensures only `web/` uploads — never the
Rust workspace or `target/`.

## Every deploy

The WASM bundles are built **locally** (Vercel has no Rust toolchain):

```bash
crates/wasm/build-web.sh      # builds web/vendor/ + web/vendor-pdf/
vercel deploy --prod          # uploads web/ and serves it
```

`build-web.sh` produces:

| Path | Bundle | Loaded |
|---|---|---|
| `web/vendor/`     | light editor engine (HTML pipeline) | on page load (~1 MB gzip) |
| `web/vendor-pdf/` | Typst PDF engine (size-optimized)   | lazily, on first PDF export (~7 MB brotli) |

## Notes

- **HTTPS is required** for the app to work (localStorage is fine, but the
  drag-and-drop and any future OAuth need a secure context). Vercel serves HTTPS
  by default.
- **No COOP/COEP headers** are set — the WASM is single-threaded, so cross-origin
  isolation isn't needed. Add them only if a future multi-threaded Typst build
  wants `SharedArrayBuffer`.
- The WASM filenames aren't content-hashed, so `vercel.json` caches them for an
  hour with revalidation (a redeploy won't serve a stale engine). For long-term
  immutable caching, hash the filenames in `build-web.sh` and update the imports.
- Custom domain: `vercel domains add ynote.onl`, then assign it to the project.
