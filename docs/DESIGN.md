# Solv: design system

The visual language for Solv's three pages. Page behaviour and API shapes come
from `docs/SPEC.md` §7; this document covers what the interface looks like and
why.

---

## 1. Direction

**Swiss Neo-Brutalist Audit Ledger.** Institutional financial-document precision
crossed with developer telemetry clarity.

The brief it answers: Solv is infrastructure that decides whether money moves.
The interface has to read as a record, not as a chat product. Three rules follow
from that, and they override any aesthetic preference:

1. **Numbers are the subject.** FICO score, latency, amount, hash. Everything
   else is annotation around them.
2. **A decision is never shown without its reasons.** This is `docs/SPEC.md` G5
   expressed in layout: there is no screen state where a verdict appears alone.
3. **Nothing implies more certainty than the data has.** A `null` score renders
   as `UNRATED`, never as a dash that reads like zero, and never as a gauge
   needle resting at the bottom.

Deliberately avoided: gradient-purple AI aesthetics, floating glass cards,
animated orbs. Not on taste grounds — they signal "demo", and Solv's entire
argument is that it is a record.

### 1.1 References

| Reference | What is borrowed |
| --- | --- |
| Stripe Docs, Stripe Press | Sharp typographic hierarchy; the two-column split of input left, live output right |
| Linear, Raycast | 1px borders, precise grid, responsive status indicators |
| Mercury, Ramp | The register of serious B2B credit tooling; spend-control framing |
| `PhiBao/tranche` (SERV Hackathon 02) | The split between a decision room and a shareable public receipt page |

---

## 2. Colour

One theme, light only. The palette is taken from reppo.xyz's published
stylesheet (2026-11-16): warm paper background, dark brown ink, an orange accent
and a 12 px radius. There is no dark theme in v2.

### 2.1 Light palette

| Role | Hex | Use |
| --- | --- | --- |
| Background | `#F9F7F6` | Page |
| Surface | `#EFEDEB` | Cards and panels: one solid fill, no border |
| Wash | `#EFEDEB` | Header, footer, quiet sections. A card inside a wash section takes the page colour, so a section shows two fills at most. |
| Ink | `#332D28` | Primary text, primary button fill |
| Muted | `#5A5049` | Secondary text, labels, units |
| Accent | `#DC5318` | Orange. Large shapes, diagram strokes, large display type. |
| Accent ink / fill | `#B8420F` | Small accent text, links, buttons with white text |

**Flat rule.** No borders, no shadows, no gradients and no translucent tints on
cards, panels, badges or inputs. A shape is told apart by its fill and its
spacing. Focus is shown with an outline, never a border. The one exception to
single fills is the five-step band scale, which uses solid hex steps of the
accent because a scale needs five distinguishable values.

Radius is `0.75rem` on cards, buttons and panels. Page containers are at most
`80rem` wide with `1.25rem` side padding, `2rem` from 640 px up.

### 2.2 Contrast: computed, not assumed

Ratios computed against WCAG 2.2. AA needs 4.5:1 for normal text, 3:1 for large
text (at least 18.66 px bold or 24 px) and for non-text UI.

| Pair | Ratio | Verdict |
| --- | --- | --- |
| `#332D28` on `#F9F7F6` | 12.71:1 | Passes AAA |
| `#332D28` on `#EFEDEB` | 11.63:1 | Passes AAA |
| `#5A5049` on `#F9F7F6` | 7.34:1 | Passes AAA |
| `#5A5049` on `#EFEDEB` | 6.71:1 | Passes AA |
| `#F9F7F6` on `#332D28` (primary button) | 12.71:1 | Passes AAA |
| `#DC5318` on `#F9F7F6` | **3.71:1** | **Fails AA for normal text** |
| `#FFFFFF` on `#DC5318` | **3.97:1** | **Fails AA for normal text** |
| `#B8420F` on `#F9F7F6` | 5.14:1 | Passes AA |
| `#B8420F` on `#EFEDEB` | 4.70:1 | Passes AA |
| `#FFFFFF` on `#B8420F` | 5.49:1 | Passes AA |
| `#FFFFFF` on `#166534` (APPROVE fill) | 7.13:1 | Passes AAA |
| `#FFFFFF` on `#B91C1C` (BLOCK fill) | 6.47:1 | Passes AA |
| `#332D28` on `#F2B13C` (HOLD fill) | 7.20:1 | Passes AAA |
| `#166534` text on `#EFEDEB` | 6.11:1 | Passes AA |
| `#B91C1C` text on `#EFEDEB` | 5.54:1 | Passes AA |
| `#92400E` text on `#EFEDEB` | 6.07:1 | Passes AA |

The constraint this creates is binding:

- `#DC5318` may be used for large display type, icons, diagram strokes and fills,
  and 2 px underlines, all of which only need 3:1.
- `#DC5318` may not be used for body text, labels or links, nor as a button fill
  behind white text. Use `#B8420F` for those.
- Status fills are solid. White text sits on green and red, ink text on amber.
  Do not pair a status colour with a pale tint of itself; that pairing is what
  the flat rule removes.

### 2.4 Decision colours

Semantic colour is the only colour that carries meaning. It maps one-to-one onto
the three decision values in `docs/SPEC.md` §4.3.

| Decision | Text | Background | Also means |
| --- | --- | --- | --- |
| `APPROVE` | `#FFFFFF` | `#166534` | `EXCEPTIONAL`/`VERY_GOOD` tier |
| `HOLD` | `#332D28` | `#F2B13C` | `UNRATED`, cold start, cap exceeded |
| `BLOCK` | `#FFFFFF` | `#B91C1C` | `POOR`, injection detected |

Colour is never the only signal. Every decision badge carries its word, and
every reason row carries its code. A red badge with no text fails for colour-blind
users and fails equally in a screenshot pasted into a bug report.

---

## 3. Typography

Two faces, both IBM Plex, matching reppo.xyz. Both are SIL Open Font License, so
they are self-hosted and redistributed; they are recorded in
`THIRD_PARTY_NOTICES.md`. The CSP forbids a font CDN, so the files ship inside the
binary.

| Face | Role | Weights |
| --- | --- | --- |
| **IBM Plex Sans** | Headings, body, labels, buttons | 400, 500, 600, 700 |
| **IBM Plex Mono** | Every numeral that is data: FICO, latency, USD, hashes, JSON, reason codes | 400, 500 |

The monospace rule is strict and worth stating as a rule rather than a
preference: **any number a user might compare against another number is set in
IBM Plex Mono.** `FICO 410 / 850`, `142ms`, `$50.00`, a SHA-256 digest. Alignment
is what makes a ledger readable.

Scale, 1.25 ratio from a 16px base:

| Token | Size / line | Use |
| --- | --- | --- |
| `display` | 48 / 52 | FICO score on the receipt page |
| `h1` | 31 / 38 | Page title |
| `h2` | 25 / 32 | Section heading |
| `h3` | 20 / 28 | Card title |
| `body` | 16 / 24 | Reason text, descriptions |
| `small` | 14 / 20 | Labels, units, metadata |
| `micro` | 12 / 16 | Reason codes, hashes, timestamps |

---

## 4. Routing: three pages

Three routes. Each page links forward to exactly where a reader needs to go
next. *(2026-11-15: with the third page live, a minimal header nav — desk,
bureau — was added at the owner's direction for cross-page discoverability;
forward links stay, and the header stays to two words.)*

### 4.0 Routing (Frontend v2, 2026-11-15)

The navbar is the sitemap: How it works · See it work · Explorer · Docs · Pricing
· Try the desk. There is no dropdown; Attack Lab is linked from How it works and
the footer. `/` holds the landing page; the desk lives at `/desk` (a form
is the demo, not the front door); the registry and receipts live at
`/explorer` — the word "bureau" read as institutional jargon and was retired
from the UI, with `/bureau` kept as a redirect so existing links survive. Every
route serves the same embedded shell under the same CSP.

### 4.1 `/desk` — The Underwriting Desk

The simulator. A visitor must understand what Solv does within five seconds of
landing, without reading a paragraph.

**Hero metric strip** — three numbers across the top, each computed from real
receipts and each carrying a `docs/SPEC.md` §9 verification label:

| Metric | Source | Label shown |
| --- | --- | --- |
| Total fraud blocked (USD) | Sum of `amount_usd` over `BLOCK` receipts | MEASURED |
| Median Tier 1 latency | p50 from stored receipts | MEASURED |
| Cost per check | Computed from token counts and published prices | CITED |
| Cost per check, until receipts record token counts | renders `—` with the word `no data`, labelled UNPROVEN — a cost figure without token counts would be a constant dressed up as a measurement | UNPROVEN |

If a metric has no data yet it renders as `—` with the word `no data`. It never
renders a placeholder number. A hero metric that is secretly a constant is the
fastest way to lose a technical reader.

**Left column — Pre-Spend Simulator.** Three one-click presets plus a custom
form (counterparty URL, amount in USD, service description):

| Preset | Demonstrates |
| --- | --- |
| Prime Verified API | `APPROVE` on the Tier 1 path alone; the fast, boring, 90% case |
| Prompt Injection Scam | `BLOCK` via `SOLV-INJECTION-001`, with the offending span quoted |
| Cold-Start New Agent | `HOLD` via `SOLV-COLDSTART-001`; honest about having no score |

The third preset is the one that earns credibility. A demo where every input
produces a confident verdict is a demo nobody believes.

**Right column — Live Decision Pipeline.** Node-by-node, each node showing its
real elapsed time:

```
State lookup       0ms    ✓
Clef-flash        44ms    ✓  injection 0.97
SERV Reasoning      —     ·  skipped: injection certain
Policy Gate        1ms    ✓  BLOCK
                                → Open Public Trust Receipt ↗
```

A skipped node renders as skipped **with its reason**, never hidden. Hiding it
would misrepresent the architecture, and the architecture is the pitch.

### 4.2 `/receipt/:id` — Public Trust Receipt

A shareable audit record, styled as a banking certificate: ledger-edge rule
lines, a monospace SHA-256 evidence hash, no interactive controls.

Order down the page:

1. **Status stamp.** `BLOCKED — $50.00 NOT AUTHORISED`,
   `HELD — $50.00 NOT AUTHORISED`, or `APPROVED — $50.00`. Large, semantic
   colour, with the word. Directly beneath it, one standing sentence on every
   receipt: "Solv's recorded decision for this request. Not an authorisation to
   pay." Solv returns a decision and never moves money, so it cannot know
   whether a payment happened; the stamp must not say "spent" or "not spent".
2. **FICO gauge**, 300–850, showing `fico_effective` with `fico_raw` and `decay`
   annotated beside it. When the score is `null`, the gauge is replaced by the
   word `UNRATED` — the needle never sits at 300 to represent "unknown".
3. **Score provenance.** Reppo `evalId`, settled-at timestamp, age in days,
   evidence basis, peer assessment count. A receipt resting on 40-day-old
   evidence says 40 days, in the same type size as the score.
4. **Four pillars**, 0–10 each, with the Reppo critique quoted beneath each one.
5. **Reasons.** Every element of `reasons[]` as a row: code in mono, message in
   IBM Plex Sans, the evidence quote in a solid block marked
   `untrusted counterparty text`.
6. **Evidence hash** plus the `canonical_json` recipe, so a third party can
   recompute it.

Security constraint, non-negotiable: `reasons[].evidence.quote` is attacker-
controlled text rendered on a public page. It is HTML-escaped, never rendered as
HTML or Markdown, capped at 240 characters, and served under a strict CSP with no
inline event handlers. `docs/SPEC.md` §8.2 lists this as a tracked threat.

### 4.3 `/bureau` — Credit Bureau Directory

1. **Directory table.** Every assessed counterparty: name, tier badge,
   `fico_effective`, evidence age, last decision. Sortable by score and by age.
2. **Learned Guardrails panel.** *(2026-11-15: rows show the rule question,
   effect, threshold, and the born-from quote; the trigger count from the
   first draft is omitted until such a counter exists in the data model —
   rendering one would be a fabricated measurement.)* Rules from `docs/SPEC.md` §5.5, split into
   `active` and `shadow`. Each row shows the rule question, its effect, trigger
   count, and the Reppo critique quote it was born from. `shadow` rules are
   visibly labelled as not yet enforcing.
3. **Reppo quota meter.** Credits remaining out of 1,000. This is a real
   constraint on the product and showing it is more honest than hiding it.

What this page must not do: claim a false-approval-rate improvement. That chart
needs a labelled ground-truth set to be anything other than decoration. Until
the Attack Lab (`docs/SPEC.md` §8.3) provides one, the page shows the Attack Lab
pass rate instead — a real number with a real denominator.

---

## 5. Components

| Component | Construction |
| --- | --- |
| **Card** | One solid fill (`#EFEDEB` on the page, `#F9F7F6` inside a wash section), 12 px radius, no border, no shadow. Emphasis cards are solid ink with page-coloured text. |
| **Decision badge** | Solid fill from §2.4 with its text colour, no border, 6 px radius, uppercase IBM Plex Sans 500, always with its word |
| **Metric tile** | Mono numeral at `display` or `h1`, `small` muted label above, verification label as `micro` below |
| **Pipeline node** | Row: name (IBM Plex Sans), elapsed ms (mono, right-aligned), state glyph, optional detail. Skipped nodes are `#5C5B57` with their reason. |
| **Reason row** | Code (mono `micro`), message (IBM Plex Sans `body`), effect chip, evidence block if present |
| **Evidence block** | Solid `#F9F7F6` fill inside a card, no border, mono, prefixed by the label `untrusted counterparty text` |
| **FICO gauge** | Horizontal track 300→850, tick marks at the four band floors (580, 670, 740, 800), filled to `fico_effective`, `fico_raw` marked with a hairline when decay is nonzero |
| **Primary button** | Ink `#332D28` fill, `#F9F7F6` text, 12 px radius |
| **Focus ring** | 2px `#D95D39` offset 2px. Never removed. |

### 5.1 Motion

Motion is used only to show that work happened, never for decoration.

- Pipeline nodes resolve in sequence, 120ms each, as their real timings arrive.
- No animation exceeds 200ms.
- Everything respects `prefers-reduced-motion: reduce` by resolving instantly.

A latency number that animates for longer than the latency it reports would be
self-defeating.

---

## 6. Frontend implementation

React + TypeScript (built with Vite), styled with Tailwind CSS compiled at
build time, embedded directly into the Rust binary:

```rust
const SPA_INDEX: &str = include_str!("../../frontend/dist/index.html");
```

Four consequences worth the constraint:

1. One deployable artefact. No Node process at runtime; `pnpm build` runs at
   development/build time only, and its output (`spa.js`, `spa.css`, fonts)
   is what the binary embeds.
2. No external request on first paint, so the page's own load time does not
   undercut a product whose claim is speed.
3. The page can be served from memory, which keeps it off the hot path's
   critical resources.
4. The desk stays reviewable: the API contract lives in typed structs
   (`frontend/src/types.ts`) mirroring SPEC §7, and the embedded output is a
   fixed set of flat files in `src/web/assets.rs`.

Decisions that follow from it:

- **Tailwind is compiled to a single static CSS file at build time, not
  loaded from a CDN.** A CDN script would add an uncached third-party
  round-trip to a page that advertises sub-200ms decisions.
- **Fonts are self-hosted woff2, subset to Latin, embedded with
  `include_bytes!`** (SIL OFL; see `THIRD_PARTY_NOTICES.md`). Google Fonts
  would work but adds DNS lookups and a third-party dependency to the
  critical path.
- **React over Alpine.js/HTMX** for the simulator (owner's decision for this
  codebase): the pipeline view is client-side state over one JSON response
  rather than a series of partial fetches. One `POST /api/v1/pre-spend-check`,
  then render nodes from the `latency_ms` and `reasons[]` fields already in
  the payload.
- **The receipt page uses no JavaScript at all.** It is server-rendered,
  which is the correct choice for an audit artefact: it renders identically
  with scripting disabled, and it has nothing to make interactive.

### 6.1 Content Security Policy

The receipt page renders untrusted text, so CSP is a functional requirement
rather than hardening:

```
default-src 'self';
script-src 'self';
style-src 'self';
img-src 'self' data:;
font-src 'self';
connect-src 'self';
frame-ancestors 'none';
base-uri 'none';
form-action 'none'
```

No `unsafe-inline` anywhere. This is why Tailwind is precompiled and Alpine is
loaded as a file rather than inlined.

---

## 7. Accessibility floor

Non-negotiable, and checked before the web PRs (PR-11 to PR-13) merge:

- Contrast meets the §2.3 table. The terracotta constraint is enforced.
- Decision state is never conveyed by colour alone.
- Every interactive element is keyboard reachable with a visible focus ring.
- The FICO gauge has a text equivalent adjacent to it; it is not the only way to
  read the score.
- The directory table uses real `<table>` semantics with `<th scope>`, not divs.
- Form inputs have associated `<label>` elements, not placeholder-only labels.
- Tables and code blocks scroll inside their own `overflow-x: auto` container.
  The page body never scrolls horizontally.
