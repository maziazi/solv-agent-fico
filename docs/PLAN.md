# Solv: work plan

This document and `docs/SPEC.md` are the project's source of truth. The plan
lives here; product detail lives in SPEC. Development order and the PR map live
in `docs/WORKFLOW.md`.

Solv is a standalone project, not tied to a hackathon deadline. SERV Hackathon
02 (Reppo track) ran 2–15 November 2026 and closed on 15 November; Edition 03 is
unannounced. Milestones below are therefore paced in days from the start date,
not against a submission date. If an Edition 03 opens, M0–M5 are the minimum
that must already be merged before the project is worth submitting.

Start date: 2026-11-15.

## Feature ownership

This is a solo project. There is no partner and no reviewer, so the column that
matters is not who owns a feature — it is whether a feature can be handed to a
subagent without the handoff costing more than it saves.

A feature is **delegable** when its interface is already pinned by SPEC and its
tests run offline. A feature is **core** when getting it wrong silently breaks a
guarantee, which means it needs the full context of the decision, not a task
brief.

| Feature | Class |
| --- | --- |
| Rust scaffold, `.env` loading, `GET /healthz` | core |
| CI/CD workflows, commit-message check (`docs/WORKFLOW.md`) | delegable |
| State layer: DashMap + atomic JSON snapshot (SPEC §5.6) | delegable |
| Score mapping and tiers (SPEC §4.2, §4.3) | delegable |
| Policy gate, deterministic (SPEC §5.4) | **core** |
| Reason taxonomy and `Reason` schema (SPEC §6) | **core** |
| Tier 1 Clef-flash client (SPEC §5.1) | delegable |
| Tier 2 SERV Reasoning client + prompt guard (SPEC §5.2) | **core** |
| `POST /pre-spend-check` wiring and latency budget | **core** |
| Receipt store, evidence hash (SPEC §6.3) | delegable |
| Reppo worker: single-slot queue, polling (SPEC §5.3) | delegable |
| Learned-rule distillation and compilation (SPEC §5.5) | **core** |
| Web: Underwriting Desk, Receipt page, Bureau page (`docs/DESIGN.md`) | delegable |
| Attack Lab corpus (SPEC §8.3) | delegable |
| Benchmarks and the measured-claims pass (SPEC §9) | **core** |
| README, demo video, deploy | delegable |

Five items are core on purpose: the policy gate, the reason taxonomy, the Tier 2
client, the latency budget, and the claims pass. Those five are where the two
flagship axes (SPEC §2) and all five guarantees actually live. They are also the
five where a plausible-looking implementation can pass its tests while the
guarantee no longer holds — which is exactly the failure a delegated task brief
cannot catch.

### Working solo: what changes

- **No reviewer means CI is the only gate.** Every check in
  `docs/WORKFLOW.md` §4.2 has to stay required, and none may be bypassed with
  `--admin`. The branch protection settings for a solo repo are in §4.2.
- **Self-review is a separate step, not a feeling.** Read the full diff in the
  GitHub PR view, not in your editor. A different surface catches different
  mistakes.
- **The PR map (`docs/WORKFLOW.md` §3) replaces coordination.** Its value solo is
  not avoiding collisions with a teammate; it is keeping a half-finished Tier 2
  client from blocking a receipt store that never needed it.
- **One context switch at a time.** With sixteen PRs and one person, the real
  risk is three open branches that each need the other two finished.

## Milestones

A milestone is done when its PRs have merged to `main` with CI green — not when
the code exists on a branch.

| # | Done when | Target | PRs (`docs/WORKFLOW.md` §3) |
| --- | --- | --- | --- |
| 0 | Repo builds, CI green on a PR, `GET /healthz` returns 200, `.env.example` complete, commit-message check rejects a bad message | 2026-11-16 | PR-01, PR-16 |
| 1 | State survives restart: profiles and receipts reload from `solv_state.json`; an interrupted write leaves no corrupt file; `GET /directory` serves real data | 2026-11-18 | PR-02 |
| 2 | Score mapping and the policy gate pass unit tests, including every row of the SPEC §5.4 table and the G1 fail-closed test | 2026-11-21 | PR-03, PR-04 |
| 3 | `POST /pre-spend-check` returns a real decision from a real Clef-flash call, with a populated `reasons[]`; Tier 1 latency recorded per request | 2026-11-24 | PR-05, PR-06 |
| 4 | Tier 2 escalation works against the real SERV endpoint; `serv_prompt_guard` refusal maps to `SOLV-INJECTION-001`; the G2 latency-budget test passes | 2026-11-27 | PR-07 |
| 5 | A receipt exists for every decision with a recomputable `evidence_sha256`; `GET /receipt/:id` returns 200 for every receipt ever issued | 2026-11-29 | PR-08 |
| 6 | One real Reppo evaluation reaches `SETTLED` and updates a profile; queue respects the single-slot limit; a learned rule is born `shadow` from a real critique | 2026-12-03 | PR-09, PR-10 |
| 7 | Three pages live and usable: Underwriting Desk, Public Trust Receipt, Bureau Directory | 2026-12-06 | PR-11, PR-12, PR-13 |
| 8 | Attack Lab: zero `APPROVE` across the corpus; `cargo bench` produces measured p50/p95; every claim in SPEC §9.1 upgraded from UNPROVEN or deleted | 2026-12-08 | PR-14, PR-15 |

Milestone 6 is the only one that depends on a third party's clock. Reppo settles
no faster than ten minutes and accepts one pending evaluation at a time (SPEC
§5.3), and the free quota is ten evaluations total. Plan on spending at most
four of those ten during development; keep the rest for the demo.

## Before coding starts

1. Three API credentials exist and are stored only in a local `.env`:
   - `CLOUDFLARE_ACCOUNT_ID` and `CLOUDFLARE_API_TOKEN` — Cloudflare dashboard,
     AI → Workers AI, token with Workers AI read/run permission
   - `SERV_API_KEY` — `console.openserv.ai`
   - `REPPO_API_KEY` — `eval.reppo.xyz`, API Key section
2. The same four values are added as GitHub Actions Secrets, so the deploy
   workflow can inject them without any key touching the repository.
3. One throwaway Clef-flash call and one throwaway SERV call succeed from the
   command line. Verifying auth before writing a client saves a day of debugging
   the wrong layer.
4. Reppo credit balance is recorded in `docs/PLAN.md` here when first checked, so
   consumption is traceable: **unverified at time of writing — check before M6.**

## Rules

- Work on a branch; merge to `main` only through a PR with CI green.
- Never push to `main`, never force-push, never merge with `--admin`. Working
  solo makes this stricter, not looser: there is nobody else to catch what a
  bypass lets through.
- No key ever enters the repository. `.env` is gitignored from PR-01 onward, and
  keys are never pasted into issues or PR descriptions.
- `cargo test` must pass with the network disabled. Any test that needs a live
  provider is `#[ignore]` and named in its PR description.
- Reppo evaluations are spent only with explicit approval, counted, and recorded.
- No claim ships above its evidence level (SPEC §9). The M8 claims pass exists
  specifically to delete claims that never got measured.

## Cut from the original plan

These were in the first sketch of Solv and are deliberately out of scope. They
are listed so that scope creep is a decision rather than a drift.

- **SQLite.** Replaced by DashMap plus an atomic JSON snapshot. The reason is the
  state-read budget in SPEC §2.1 and keeping deployment to one binary. The cost
  is accepted and documented in SPEC §5.6.
- **Reppo in the hot path.** Impossible, not merely expensive: 10-minute minimum
  settlement, one pending evaluation at a time. It became the scheduled worker in
  SPEC §5.3. Any pitch that implies an instant third-party audit is a false
  claim.
- **`serv_shadow_agent` on the hot path.** Non-streaming and a full extra
  validation round. Kept for the Tier 3 batch, where latency is irrelevant.
- **On-chain payment-conduct data.** `agentfico` already does this with Kite and
  does it better. Adding a weak version would blur Solv's one axis of
  differentiation (SPEC §1.3) rather than strengthen it.
- **Auto-promoting learned rules to `active`.** Rules are born `shadow` and
  require operator approval (SPEC §5.5). A model-generated rule that blocks
  payments unsupervised is not a risk worth taking at this stage.
- **A generic multi-tenant API with billing.** The revenue model in SPEC §10 is
  an argument, not a build item. Metering, invoicing, and tenant isolation are
  out of scope until someone asks to pay.
- **The "40x cheaper" headline.** Withdrawn, not postponed. It cannot be derived
  from any published price; see SPEC §10.3 for the figures that can.
