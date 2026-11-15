# Third-party notices

Fonts embedded in the web frontend (`frontend/public/fonts/`) and served
self-hosted from `/static/fonts/`. Both families are licensed under the SIL
Open Font License 1.1, which permits bundling and redistribution as part of a
larger work. They are latin-subset woff2 files, one per weight.

| Font | Weights shipped | Copyright | License |
| --- | --- | --- | --- |
| IBM Plex Sans | 400, 500, 600, 700 | 2019 IBM Corp. | SIL OFL 1.1 |
| IBM Plex Mono | 400, 500 | 2017 IBM Corp. | SIL OFL 1.1 |

Font files come from the `@fontsource/ibm-plex-sans` and
`@fontsource/ibm-plex-mono` packages (version 5.3.0), which repackage the
upstream IBM Plex release, downloaded on 2026-11-16. The full OFL 1.1 text
ships with the fonts: it is embedded as an asset and served at
`/static/fonts/OFL-1.1.txt` (and lives in `frontend/public/fonts/OFL-1.1.txt`).

Nothing else in this repository is third-party code. The frontend build
toolchain (Vite, React, Tailwind) is a dev-time dependency only — none of it
ships inside the binary; it compiles to the embedded `spa.js`/`spa.css`.
