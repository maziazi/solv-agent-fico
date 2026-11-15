# Solv desk — frontend

React 19 + TypeScript + Tailwind CSS 4, built by Vite. The build output in
`dist/` is committed because the Rust binary embeds it at compile time
(docs/DESIGN.md §6) — `cargo build` needs no Node toolchain.

```sh
pnpm install
pnpm build        # writes dist/ (spa.js, spa.css, fonts/) — rebuild before cargo build
pnpm dev          # Vite on :5173, proxying /api and /receipt to 127.0.0.1:8080
```

Layout: `src/pages/` one file per route, `src/components/` the shared pieces
(badges, pipeline, reasons, score panel, site chrome), `src/types.ts` mirrors
the SPEC §7 API structs, `src/api.ts` is the only file that fetches.

Rules inherited from the repo (AGENTS.md, docs/DESIGN.md): no external
request on first paint (fonts are self-hosted in `public/fonts/`), no
`unsafe-*` CSP anywhere, numerals are data and always set in JetBrains Mono,
and a metric without data renders "no data" — never a placeholder number.
