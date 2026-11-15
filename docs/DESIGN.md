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

Two themes. Light is the default; it is the one that reads as a document.

### 2.1 Warm Ledger (light)

| Role | Hex | Use |
| --- | --- | --- |
| Background | `#F8F7F4` | Page. Warm alabaster, audit paper. |
| Surface | `#FFFFFF` | Cards and panels |
| Border | `#E2E0D8` | 1px, solid, everywhere. The primary structural device. |
| Ink | `#141413` | Primary text, all numerals |
| Muted | `#5C5B57` | Secondary text, labels, units |
| Accent | `#D95D39` | Terracotta. See the constraint in §2.3. |
| Accent fill | `#A8431F` | Darkened terracotta for buttons with white text |

### 2.2 Obsidian Ledger (dark)

| Role | Hex | Use |
| --- | --- | --- |
| Background | `#12110F` | Page |
| Surface | `#1A1917` | Cards and panels |
| Border | `#2E2C28` | 1px, solid |
| Ink | `#F4F2ED` | Primary text, all numerals |
| Muted | `#A3A09A` | Secondary text |
| Accent | `#E8734A` | Lifted terracotta; `#D95D39` is too dark on this background |

### 2.3 Contrast: measured, not assumed

Ratios computed against WCAG 2.2. AA needs 4.5:1 for normal text, 3:1 for large
text (≥18.66px bold or ≥24px) and for non-text UI.

| Pair | Ratio | Verdict |
| --- | --- | --- |
| `#141413` on `#F8F7F4` | ~17:1 | Passes AAA |
| `#5C5B57` on `#F8F7F4` | ~6.1:1 | Passes AA |
| `#15803D` on `#DCFCE7` | 4.56:1 | Passes AA, narrowly |
| `#B91C1C` on `#FEE2E2` | 5.29:1 | Passes AA |
| `#B45309` on `#FEF3C7` | 4.51:1 | Passes AA, narrowly |
| `#D95D39` on `#FFFFFF` | **3.77:1** | **Fails AA for normal text** |
| `#FFFFFF` on `#D95D39` | **3.77:1** | **Fails AA for normal text** |
| `#FFFFFF` on `#A8431F` | 6.01:1 | Passes AA |

The constraint this creates, and it is binding:

- `#D95D39` may be used for **1px borders, large display type, icons, and focus
  rings** — all of which only need 3:1.
- `#D95D39` may **not** be used for body text, labels, links, or as a button
  fill behind white text. Buttons use `#A8431F`.

The three narrow passes (4.51 and 4.56) mean the semantic text colours must not
be lightened. If a future change wants a softer green, the background has to
darken to compensate.

### 2.4 Decision colours

Semantic colour is the only colour that carries meaning. It maps one-to-one onto
the three decision values in `docs/SPEC.md` §4.3.

| Decision | Text | Background | Also means |
| --- | --- | --- | --- |
| `APPROVE` | `#15803D` | `#DCFCE7` | `PRIME` tier, FICO ≥ 720 |
| `HOLD` | `#B45309` | `#FEF3C7` | `UNRATED`, cold start, cap exceeded |
| `BLOCK` | `#B91C1C` | `#FEE2E2` | `HIGH_RISK`, injection detected |

Colour is never the only signal. Every decision badge carries its word, and
every reason row carries its code. A red badge with no text fails for colour-blind
users and fails equally in a screenshot pasted into a bug report.

---

## 3. Typography

Three faces, each with one job. All three are SIL Open Font License, so they can
be self-hosted and redistributed; record them in `THIRD_PARTY_NOTICES.md` when
the first font file lands.

| Face | Role | Weights |
| --- | --- | --- |
| **Space Grotesk** | Headings, page titles, decision stamps | 500, 700 |
| **Inter** | Body, reason text, labels, UX copy | 400, 500 |
| **JetBrains Mono** | Every numeral that is data: FICO, latency, USD, hashes, JSON, reason codes | 400, 500 |

The monospace rule is strict and worth stating as a rule rather than a
preference: **any number a user might compare against another number is set in
JetBrains Mono.** `FICO 410 / 850`, `142ms`, `$50.00`, a SHA-256 digest. Tabular
figures make columns align, and alignment is what makes a ledger readable.

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

Three routes. No navigation menu — each page links forward to exactly where a
reader needs to go next.

### 4.1 `/` — The Underwriting Desk

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
   Inter, the evidence quote in a bordered block marked
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
2. **Learned Guardrails panel.** Rules from `docs/SPEC.md` §5.5, split into
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
| **Card** | `#FFFFFF` surface, 1px `#E2E0D8` border, 0 radius or 2px max, no shadow. Depth comes from borders. |
| **Decision badge** | Semantic background, semantic text, 1px border of the same text colour, uppercase Space Grotesk 500, always with its word |
| **Metric tile** | Mono numeral at `display` or `h1`, `small` muted label above, verification label as `micro` below |
| **Pipeline node** | Row: name (Inter), elapsed ms (mono, right-aligned), state glyph, optional detail. Skipped nodes are `#5C5B57` with their reason. |
| **Reason row** | Code (mono `micro`), message (Inter `body`), effect chip, evidence block if present |
| **Evidence block** | `#F8F7F4` fill, 1px dashed border, mono, prefixed by the label `untrusted counterparty text` |
| **FICO gauge** | Horizontal track 300→850, tick marks at the four tier floors (550, 640, 720), filled to `fico_effective`, `fico_raw` marked with a hairline when decay is nonzero |
| **Primary button** | `#A8431F` fill, white text, 1px darker border |
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
