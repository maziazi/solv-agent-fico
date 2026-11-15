# Frontend v2 — landing page, navigation, and the public explorer

Status: **PARTIALLY IMPLEMENTED** (2026-11-15): the landing page, navbar, desk move to `/desk`, explorer at `/explorer`, content pages, and band legend shipped; `/agent/:id` pages, the search bar, and the remaining §5 pages are future work. `docs/DESIGN.md` remains the binding
visual system; this document extends it with new pages and does not replace its
colour, typography, or accessibility rules.

What v1 is: two pages (`/` desk, `/bureau`) plus `/receipt/:id`. A visitor lands
directly on a form with no statement of what Solv is, no integration example,
and no route for anyone who is not already operating an agent.

What v2 adds: a landing page that explains the product, a navbar that makes
every feature reachable, and a public explorer with a documented read API so a
third party can check a counterparty without running the app.

---

## 1. Direction

Keep the **Swiss Neo-Brutalist Audit Ledger** register from `docs/DESIGN.md` §1:
reppo-style light palette (docs/DESIGN.md §2), IBM Plex Mono for every number, decision never
shown without reasons, `UNRATED` rendered as a word.

Borrow **structure** — not palette — from three references:

| Reference | What is borrowed |
| --- | --- |
| `stripe.com/radar` | Landing page architecture: lifecycle framing, grouped capability blocks, a stat row, integration options, audience segment tabs, FAQ, closing CTA |
| `reppo.xyz/eval` | Section rhythm: numbered steps (01 / 02 / 03), card-based stat displays, a receipts table as a first-class section, FAQ near the end |
| Block explorers (Etherscan, NEARBlocks) | Explorer IA: persistent search, stat tiles, "latest activity" lists, stable per-entity URLs, a detail page whose subject is the hash |

Note on the explorer reference: `nearblocks.io` returned HTTP 403 when fetched
for this document, so what is described below is the standard explorer pattern
rather than a verified reading of that specific site. Confirm details against
the live site before implementing.

The three rules from `docs/DESIGN.md` §1 override every borrowed pattern. In
particular: Stripe Radar's landing page makes outcome claims without evidence on
the page itself. Solv must not. Every number on the Solv landing page carries a
`PROVEN` / `MEASURED` / `UNPROVEN` label, as the metric tiles already do.

---

## 2. Navigation

A single persistent navbar on every page. Current nav has two items and hides
most of the product.

```
┌────────────────────────────────────────────────────────────────────────┐
│ ■ Solv   Product   How it works   Explorer   Docs   Pricing   [Try the │
│          ▾                                                      desk]  │
└────────────────────────────────────────────────────────────────────────┘
```

| Item | Route | Contents |
| --- | --- | --- |
| **Solv** (wordmark) | `/` | Landing page |
| **Product** ▾ | — | Dropdown: Pre-spend gate · Credit bureau · Learned guardrails · Attack Lab |
| **How it works** | `/how-it-works` | The three tiers, the decision table, the score pipeline |
| **Explorer** | `/explorer` | Public search over counterparties and receipts |
| **Docs** | `/docs` | API reference, integration snippets, reason-code taxonomy |
| **Pricing** | `/pricing` | Cost model, labelled per claim |
| **Try the desk** (primary button) | `/desk` | The current `/` page, moved |

Two structural moves:

- **The desk moves from `/` to `/desk`.** The form is the product's demo, not its
  front door. `/bureau` becomes a tab inside `/explorer`.
- **The navbar is the sitemap.** Every feature named anywhere in the README must
  be reachable in at most two clicks from the navbar; if it is not, either the
  navbar is wrong or the feature does not exist yet.

Footer keeps the existing disclaimer and source link, and adds: SPEC, threat
model, claims, reason-code taxonomy, status.

---

## 3. Landing page (`/`)

Sections in order. The Stripe Radar structure, re-aimed at agent payments.

### 3.1 Hero

- One sentence: *"The credit check an AI agent runs before it pays."*
- One subordinate line naming the two failure modes: fake counterparties and
  prompt injection hidden in service descriptions.
- Two buttons: **Try the desk** (primary) · **Check a counterparty** (to the
  explorer).
- No illustration. A `curl` block showing a request and the `BLOCK` response,
  monospace, is the hero image.

### 3.2 "Risk is not only at the moment of payment"

Stripe's lifecycle framing, mapped to the agent payment lifecycle. Three columns:

| Stage | What goes wrong | What Solv does |
| --- | --- | --- |
| **Before the first payment** | Counterparty does not exist, URL is dead, claims are unbacked | Reppo audit produces a settled score; no score means `UNRATED`, not a guess |
| **At the moment of payment** | Prompt injection in the service description, amount above what the counterparty has earned, detectors down | Tier 1 reflex, Tier 2 escalation, deterministic policy gate, fail-closed |
| **After payment** | Evidence goes stale, a critique reveals a pattern the gate missed | Age decay, discard at 90 days, critiques distilled into shadow rules |

This section is what makes the product legible in ten seconds. It does not exist
today in any form.

### 3.3 Capability blocks

Four blocks, each a short paragraph plus a link to its deep page. Mirrors
Stripe's "unify your fraud prevention" grouping.

1. **Pre-spend gate** — `APPROVE` / `HOLD` / `BLOCK` with reasons, on the hot path.
2. **Credit bureau** — independent evaluation, five bands, age decay, no write path.
3. **Learned guardrails** — critiques become shadow rules; operator promotion only.
4. **Public receipts** — every decision gets a hash a third party can recompute.

### 3.4 Evidence row

Stripe puts four big numbers here. Solv puts four **labelled** numbers:

| Number | Label |
| --- | --- |
| 20 / 20 injection cases refused | `PROVEN` — `tests/attack.rs` |
| p95 58 µs gate-side hot path | `MEASURED` — `cargo bench`, n=1000 |
| 0 actors can write a score | `PROVEN` — no write endpoint exists |
| Receipt hash recomputable by a third party | `PROVEN` — `tools/recompute_receipt.py` |

Rule: a tile with no evidence shows the claim and the word `UNPROVEN`, never a
number. A tile with no data shows `no data`, never a zero.

### 3.5 How it works — numbered steps

Reppo's 01 / 02 / 03 rhythm, five steps, each with the component that performs it:

```
01  State lookup          in-process, sub-millisecond
02  Tier 1 reflex         Cloudflare Clef-flash, typed questions
03  Tier 2 escalation     OpenServ SERV Reasoning, only when needed
04  Policy gate           deterministic, 12 ordered rows, Rust
05  Receipt               hashed, public, permanently addressable
```

Below the steps: the decision table itself, rendered as a table. Twelve rows,
each showing the condition, the outcome, and the reason code. This is Solv's
strongest differentiator and it is currently invisible in the UI.

### 3.6 Integration

Stripe's "bring Radar to your existing stack", three options:

- **Call the gate** — `POST /api/v1/pre-spend-check`, copyable `curl` and TypeScript.
- **Read the bureau** — `GET /api/v1/agent/:id`, for platforms that score in their own system.
- **Self-host** — one Rust binary, one Docker image, no database.

### 3.7 Audience segments

Tabbed, one panel each. These are the three audiences from the README and each
needs a different first action:

| Segment | Panel content | Primary action |
| --- | --- | --- |
| **Agent operators & wallet providers** | Latency budget, fail-closed behaviour, per-band caps, self-host | Try the desk |
| **Marketplaces & bounty platforms** | Auditable screening, receipt ledger, bulk read API, policy versioning | Read the API docs |
| **API & MCP providers** | How to get rated, what the four pillars ask, what a settled score means | Request an audit |

### 3.8 FAQ

Six questions, answered honestly: What is Solv? Is the score calibrated? What
happens when a provider is down? Who can change a score? What does `UNRATED`
mean? What is not proven yet?

### 3.9 Closing CTA

Repeat of the hero buttons plus a link to the SPEC.

---

## 4. Explorer (`/explorer`)

The chosen scope is **read-only pages plus a documented public read API**.

### 4.1 Index (`/explorer`)

- **Search bar**, persistent, pinned to the navbar on explorer routes. Accepts a
  counterparty id, a receipt id (`slv_…`), or an evaluation id.
- **Stat tiles**: counterparties rated, receipts issued, decisions by type,
  `total_blocked_usd`, Reppo credits remaining. Same honesty rules as the desk.
- **Latest receipts** table: receipt id · decision · band · amount · counterparty
  · age. Rows link to the receipt page.
- **Counterparty directory** table: id · band · `fico_effective` · evidence age ·
  last decision. Sortable by score and by evidence age. This is `/bureau` today;
  it becomes a tab here.
- **Band legend** component, shared with the gauge, so the five bands and their
  caps are explained wherever a band appears.

### 4.2 Counterparty page (`/agent/:id`)

The page a provider sends to a buyer. Contents:

- Header: id, target URL, band chip, `fico_effective` on the five-band gauge.
- The four pillar scores, each 0–10, with Reppo's critique text, escaped and
  marked untrusted.
- Evidence block: `eval_id`, settled timestamp, `age_days`, `decay`, `fico_raw`.
- Decision history: every receipt naming this counterparty, newest first.
- Honest empty states: `UNRATED` renders as a word with the cold-start
  explanation, never as an empty gauge.

### 4.3 Receipt page (`/receipt/:id`)

Already exists and is the strongest page in the product. Two additions:

- A **"verify this receipt"** block: the exact `recompute_receipt.py` command with
  this receipt's id filled in, plus the expected hash. Copyable.
- Previous / next navigation within the counterparty's history.

### 4.4 Public read API (`/docs/api`)

| Endpoint | Returns |
| --- | --- |
| `GET /api/v1/agent/:id` | Band, score, pillars, evidence age, decision count |
| `GET /api/v1/receipts?limit&cursor&decision` | Paginated receipt list |
| `GET /api/v1/receipts/:id` | One receipt, identical payload to the HTML page |
| `GET /api/v1/directory` | Exists today; documented and paginated |
| `GET /api/v1/metrics` | The stat tiles, machine-readable |

Constraints that must hold:

- **Read-only.** No public route writes anything. `POST /audit-counterparty`
  stays operator-only until it has auth (SPEC §8.2, `src/bureau.rs:33`).
- The "Request an audit" action in §3.7 therefore links to a contact or form
  route, not to the write endpoint, until auth exists.
- Responses are cacheable and carry `ruleset_version`, so a consumer can tell
  which policy produced a decision.
- Every payload already rendered in HTML must be obtainable as JSON at the same
  path under `/api/v1`. No fact should be visible in the UI but unreachable by API.

---

## 5. Other pages

| Route | Purpose |
| --- | --- |
| `/how-it-works` | Long form: three tiers, the decision table row by row, the score pipeline, the fail-closed guarantees with the test names that prove them |
| `/docs` | API reference, integration snippets, the full reason-code taxonomy from SPEC §6.2, self-host instructions |
| `/pricing` | Cost model per SPEC §10, every figure carrying its claim label; the Reppo price beyond the free tier is `UNPROVEN` and must say so |
| `/attack-lab` | The six attack categories, the 20 cases, and the command that reproduces the result |

---

## 6. Component additions

Beyond what `docs/DESIGN.md` §5 already defines:

- **Navbar** with dropdown, mobile drawer, and an explorer-mode variant carrying
  the search field.
- **Search field** with id-type detection (`slv_…` → receipt, otherwise counterparty).
- **Band chip** — five-step ramp, always accompanied by the numeric score; a chip
  alone is never sufficient.
- **Band legend** — bands, ranges, caps, in one reusable block.
- **Step list** — numbered 01/02/03 with a rule and a label, per §3.5.
- **Stat tile with claim label** — generalisation of the existing `MetricTile`.
- **Code block with copy** — monospace, no syntax-colour theme that fights the
  warm palette.
- **Data table** — sortable, paginated, used by the directory and the receipt list.
- **Segment tabs** — for §3.7.
- **FAQ accordion** — keyboard accessible, open by default for the first item.

Accessibility floor from `docs/DESIGN.md` §7 applies to every one of them. The
five-band ramp in particular must be contrast-measured, not assumed, and must
never be the only channel carrying the band — the numeric score is always shown.

---

## 7. TODO

- [ ] PR-F1 `feat(web): navbar and route shell`
  - [ ] Navbar, dropdown, mobile drawer, footer expansion
  - [ ] Move the desk from `/` to `/desk`; redirect `/` to the landing page
  - [ ] Route skeletons for every navbar destination so no link is dead
- [ ] PR-F2 `feat(web): landing page`
  - [ ] §3.1–3.9 sections
  - [ ] Evidence row wired to real test results and bench output, each labelled
  - [ ] Decision table rendered from the same source of truth as the gate
- [ ] PR-F3 `feat(api): public read endpoints`
  - [ ] `/api/v1/agent/:id`, `/api/v1/receipts`, `/api/v1/receipts/:id`, `/api/v1/metrics`
  - [ ] Pagination with a stable cursor; `ruleset_version` on every payload
  - [ ] Offline tests for each shape
- [ ] PR-F4 `feat(web): explorer`
  - [ ] `/explorer` index with search, stat tiles, both tables
  - [ ] `/agent/:id` page
  - [ ] Receipt page: verify block and history navigation
- [ ] PR-F5 `feat(web): how-it-works, docs, pricing, attack-lab`
- [ ] PR-F6 `feat(web): five-band gauge, chip, and legend` — depends on
      `docs/TIERS-V2.md` PR-T1 landing first
- [ ] PR-F7 `chore(demo): seeded state for the public instance`
  - [ ] `examples/demo_state.json` from real settled evaluations
  - [ ] Demo-mode banner when no provider keys are attached, so the presets stop
        promising reason codes that cannot fire

## 8. Open questions

- Does the explorer index belong at `/explorer` or at the root, with the landing
  page at `/about`? Explorer-first suits the bureau framing; landing-first suits
  the three audiences. Current proposal: landing at `/`.
- Should `/agent/:id` show the full Reppo critique text, or only the distilled
  rule? Full text is more verifiable and more hostile input to render; it is
  already escaped and bounded, so the proposal is to show it, marked untrusted.
- Is a dark theme in scope for v2? No. v2 ships the light palette only
  (`docs/DESIGN.md` §2); a dark theme would double the surface to check.
