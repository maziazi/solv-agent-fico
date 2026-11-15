# Third-party notices

Fonts embedded in the web frontend (`frontend/public/fonts/`) and served
self-hosted from `/static/fonts/`. All three are licensed under the SIL Open
Font License 1.1, which permits bundling and redistribution as part of a
larger work. They are latin-subset variable woff2 files.

| Font | Copyright | License |
| --- | --- | --- |
| Space Grotesk | 2019 The Space Grotesk Project Authors (Florian Karsten, initial designs) | SIL OFL 1.1 |
| Inter | 2016–2020 The Inter Project Authors (Rasmus Andersson) | SIL OFL 1.1 |
| JetBrains Mono | 2020 The JetBrains Mono Project Authors (JetBrains) | SIL OFL 1.1 |

Font files were subset to Latin by Google Fonts' pipeline and downloaded from
`fonts.gstatic.com` on 2026-11-15. License texts: https://scripts.sil.org/OFL.

Nothing else in this repository is third-party code. The frontend build
toolchain (Vite, React, Tailwind) is a dev-time dependency only — none of it
ships inside the binary; it compiles to the embedded `spa.js`/`spa.css`.
