# Tiers v2 — five FICO-aligned bands

Status: **IMPLEMENTED** (this PR). `src/score.rs`, the policy gate, the
receipt page, the tests, and SPEC §4.3 now follow this document; the reason
for the §4.3 change is recorded there and in the PR description, per
AGENTS.md (the score mapping is a public promise printed on every receipt).

Goal: replace the four ad-hoc bands with five bands whose boundaries are the
published FICO boundaries, and give each band a consequence that is enforced by
the policy gate rather than described in prose.

---

## 1. Why this change

The current bands are `PRIME ≥ 720`, `STANDARD 640–719`, `SUBPRIME 550–639`,
`HIGH_RISK < 550` (`src/score.rs:168`, SPEC §4.3). Two problems:

1. **The boundaries match nothing.** Published FICO bands are 300–579, 580–669,
   670–739, 740–799, 800–850. VantageScore 3.0 bands are 300–600, 601–660,
   661–780, 781–850. Solv uses 550/640/720, which is neither, while borrowing
   the word "FICO" for the scale and `PRIME`/`SUBPRIME` from VantageScore. A
   reader who knows credit scoring reads this as carelessness.
2. **The consequences are coarse.** Three caps ($10 / $100 / $500) over four
   bands, with the top cap equal to `absolute_cap_usd`, so `PRIME` and the
   operator's hard ceiling are indistinguishable in practice.

Neither problem is about correctness of code. Both are about whether the number
can be defended.

---

## 2. The five bands

| Band | `fico_effective` | Zone | Default decision | `max_safe_spend` |
| --- | --- | --- | --- | --- |
| `POOR` | 300–579 | Red | `BLOCK` | $0.00 |
| `FAIR` | 580–669 | Watch | `APPROVE` ≤ cap, else `HOLD` | $5.00 |
| `GOOD` | 670–739 | Standard | `APPROVE` ≤ cap, else `HOLD` | $50.00 |
| `VERY_GOOD` | 740–799 | Trusted | `APPROVE` ≤ cap, else `HOLD` | $250.00 |
| `EXCEPTIONAL` | 800–850 | Priority | `APPROVE` ≤ cap, else `HOLD` | `absolute_cap_usd` (default $500.00) |
| `UNRATED` | no settled score | Cold start | `HOLD` | $0.00 |

`UNRATED` is **not** a band and must never be folded into `FAIR`. A counterparty
nobody has evaluated has no evidence, not weak evidence. Giving it a $5 ceiling
would mean inventing a trust level, which SPEC §4.2 forbids in the same words it
uses for inventing a score.

**`EXCEPTIONAL` is not unrestricted.** `SOLV-LIMIT-002` still blocks any amount
above `absolute_cap_usd`, at every band, with no exception. The band raises the
tier cap to meet the operator ceiling; it never removes a ceiling. Any UI copy
reading "unrestricted" or "no limits" is a false claim and is prohibited.

### 2.1 What the bands mean in Reppo terms

Because `fico_raw = 300 + round(reppo_score × 5.5)`, every band boundary is also
a boundary on Reppo's 0–100 verdict. This table is the one that matters
operationally, and it must be published with the one above:

| Band | `reppo_score` (real-valued) | Integer verdicts |
| --- | --- | --- |
| `POOR` | < 50.82 | 0–50 |
| `FAIR` | 50.82 – 67.17 | 51–67 |
| `GOOD` | 67.18 – 79.90 | 68–79 |
| `VERY_GOOD` | 79.91 – 90.81 | 80–90 |
| `EXCEPTIONAL` | ≥ 90.82 | 91–100 |

Two consequences worth stating before anyone is surprised by them:

- **`POOR` covers half the Reppo scale.** A verdict of 50/100 — a plain "middling"
  judgement — produces 575 and an automatic `BLOCK`. That is a deliberately
  severe default for a payment gate, but it must be a stated choice, not a
  surprise discovered in production.
- **`EXCEPTIONAL` needs a near-perfect verdict** (≥ 91). It may never be reached
  by a real counterparty. If after real evaluations nothing lands there, the
  honest fix is to say so in the README, not to quietly move the boundary.

---

## 3. What already exists

Answering the question directly: **the consequences are already enforced, just
with different numbers and one fewer band.** This proposal is a re-parameterisation
plus one new enum variant, not new machinery.

| Consequence | Implemented? | Where |
| --- | --- | --- |
| Score below floor → `BLOCK` | Yes, floor 550 | `src/policy.rs:350`, `SOLV-SCORE-001` |
| Safety pillar ≤ 3 → `BLOCK`, even on an expired score | Yes | `src/policy.rs:333`, `SOLV-SCORE-002` |
| Amount over tier cap → `HOLD` | Yes | `src/policy.rs:437`, `SOLV-LIMIT-001` |
| Amount over operator ceiling → `BLOCK` | Yes | `src/policy.rs:418`, `SOLV-LIMIT-002` |
| No settled score → `HOLD`, $0 | Yes | `src/policy.rs:401`, `SOLV-COLDSTART-001` |
| Per-band caps, operator-configurable | Yes, three caps | `src/score.rs:138`, `Caps` |
| Age decay, discard past 90 days | Yes | `src/score.rs:73` |
| Five bands on FICO boundaries | **No** | this document |
| Distinct cap per band ($0/5/50/250/max) | **No** | this document |
| Decay reason surfaced on receipt | Yes | `src/policy.rs:557`, `SOLV-SCORE-003` |

So: nothing here is a new capability. It is a boundary change, a cap change, a
fifth variant, and the honesty work that has to travel with them.

---

## 4. Changes required

### 4.1 `src/score.rs`

- `Tier` gains `Poor`, `Fair`, `Good`, `VeryGood`, `Exceptional`; drops `Prime`,
  `Standard`, `Subprime`, `HighRisk`. `Unrated` stays unchanged.
- Wire strings become `POOR`, `FAIR`, `GOOD`, `VERY_GOOD`, `EXCEPTIONAL`,
  `UNRATED`. **This is a breaking API change**; `tier_wire_strings_are_pinned`
  must be updated deliberately, never loosened.
- `Caps` gains a field per band: `fair_usd`, `good_usd`, `very_good_usd`.
  `EXCEPTIONAL` reads `absolute_cap_usd` rather than carrying its own cap, so
  the top band cannot be configured above the operator's hard ceiling.
- `tier_for` boundaries become 800 / 740 / 670 / 580.

### 4.2 `src/policy.rs`

- `PolicyConfig::block_floor` default moves 550 → 580 so the floor and the
  `POOR` ceiling are the same number. Two numbers that must agree but are
  configured separately are a bug waiting to happen; add a test that asserts
  `block_floor == FAIR_FLOOR`.
- Decision table rows are otherwise unchanged. Row 6 blocks below the floor, row
  10 compares the amount to the band cap. The band change flows through.

### 4.3 Receipts and versioning

- `ruleset_version` must increment. Receipts issued under v1 carry v1 bands and
  must keep rendering with the band names that were active when they were
  issued — a receipt whose meaning changes retroactively is not a receipt.
- `docs/SPEC.md` §4.3 is rewritten, and §4.2 gains the calibration paragraph in
  §6 below. AGENTS.md requires the reason to be written in the PR.

### 4.4 Frontend

- The FICO gauge tick marks move from 550/640/720 to 580/670/740/800
  (`docs/DESIGN.md` §5, "FICO gauge").
- Band colours need a five-step ramp. Decision colours (`APPROVE`/`HOLD`/`BLOCK`)
  stay as they are and must stay visually distinct from the band ramp, so a
  reader never confuses "what band is it" with "what happened".
- See `docs/FRONTEND-V2.md` for where bands appear in the new pages.

---

## 5. Migration

1. Land `src/score.rs` + `src/policy.rs` + tests in one PR; the band change and
   the cap change belong together because neither is meaningful alone.
2. Bump `ruleset_version` to 2 in the same PR.
3. Update the SPEC in the same PR — the SPEC is the source of truth, so it must
   never lag the code.
4. Frontend follows in its own PR; the API shape is already stable by then.
5. No state migration is needed: bands are derived at read time from
   `SettledScore`, never stored.

---

## 6. Calibration statement (required wording)

This paragraph must appear in SPEC §4.2 and in the README section that shows the
formula. Without it the bands import credibility they have not earned:

> The factor 5.5 is a range fit — (850 − 300) ÷ 100 — not a calibration. It
> rescales Reppo's verdict and adds no information; `fico_effective` is a
> monotone function of `reppo_score`, so the only thing that separates
> counterparties is the band boundary. The boundaries are taken from the
> published FICO bands so that a reader can check them against a public
> reference, not because Solv has measured default rates — it has none. The caps
> are operator defaults chosen for a conservative gate. All of it is
> **UNPROVEN** and will stay so until Solv has outcome data: counterparties that
> were approved and then failed to deliver.

Add a matching line to SPEC §11 (Unproven):

> - Band boundaries and per-band caps. They are chosen, not calibrated. No
>   false-positive or false-negative rate has been measured, because no outcome
>   data exists.

---

## 7. TODO

- [ ] PR-T1 `feat(score): five FICO-aligned bands and per-band caps`
  - [ ] `Tier` enum, wire strings, `tier_for` boundaries
  - [ ] `Caps` per band; `EXCEPTIONAL` reads `absolute_cap_usd`
  - [ ] `block_floor` default 580 + test asserting it equals the `FAIR` floor
  - [ ] Table-driven test pinning all five boundaries from both directions
        (`fico_effective` and `reppo_score`)
  - [ ] Test asserting `EXCEPTIONAL` still blocks above `absolute_cap_usd`
  - [ ] `ruleset_version` → 2
  - [ ] SPEC §4.2 calibration paragraph, §4.3 table, §11 entry
- [ ] PR-T2 `feat(web): five-band gauge and band ramp`
  - [ ] Gauge ticks 580/670/740/800
  - [ ] Five-step band ramp, contrast measured per `docs/DESIGN.md` §2.3
  - [ ] Band legend component reused by the explorer (`docs/FRONTEND-V2.md`)
- [ ] PR-T3 `docs(readme): bands, consequences, and the calibration statement`

## 8. Decisions taken

- **`FAIR` approves under $5.** Decided 2026-11-15. Not a `HOLD`-with-exception:
  a `HOLD` that an operator always releases teaches operators to ignore holds,
  which costs more than the $5 at risk. Above $5 the band holds on
  `SOLV-LIMIT-001` like every other band.
- **`UNRATED` stays separate from `FAIR`.** Decided 2026-11-15. Cold start is
  absence of evidence, not weak evidence: $0, `HOLD`, no number.

## 9. Open questions

- Should the band be recomputed when `decay` crosses a boundary mid-life, or
  pinned at settlement? Current behaviour recomputes at read time
  (`src/score.rs:96`), which is the honest choice and should stay.
- Does a band change deserve its own reason code on the receipt
  (`SOLV-SCORE-004`, "band changed since the last decision for this
  counterparty")? Useful for the explorer timeline; not required for the gate.
