# Presenting Solv

A speaker's runbook. The audience sees the web app, so the order is: landing page
(explain), then the live demo (prove), then the three audiences (who it is for).
Numbers are the measured ones from 2026-11-15 and carry their sample size. If a
number you want to say is not in section 6, do not say it.

## 0. Before you go on stage

Do these the day before, then once more 15 minutes before.

- [ ] **Deploy is finished and warm.** Never deploy during the demo. Open the
      public URL and wait for the landing page. Some free hosts sleep when idle,
      and the first request can then take about a minute.
- [ ] **The demo profile is loaded.** Open `/explorer`: `github-rest-api` must be
      listed as EXCEPTIONAL (850). If the list is empty, the service was started
      without `SOLV_SEED_FILE=seed/demo_profiles.json` and the APPROVE demo will
      show a cold-start HOLD instead.
- [ ] **Provider keys are set** on the host (Cloudflare, SERV). Without them every
      check holds on `SOLV-SYSTEM-003` by design, which is the wrong demo.
- [ ] **One dry run** of the whole path in section 2, on the same network you will
      present from. Note the times; yours replace the ones below.
- [ ] **Reppo is not needed.** The audit worker is off on the public service. Do
      not promise a live audit on stage: it takes 2 to 5 minutes and costs 440
      credits (about $0.22).
- [ ] Browser zoom at 100 percent, one tab on `/`, a second tab on `/desk`.
- [ ] The seed profile expires on about 2027-02-13 (scores age after 30 days and
      are dropped after 90). After that date `github-rest-api` is unrated.

## 1. Landing page (about 3 minutes)

Stay on `/`. Do not scroll while you talk about the first screen.

**What Solv is, in 30 seconds.** An AI agent that has a budget cannot tell a real
counterparty from a made-up one, and it reads the service text it is given as if
it were instructions. Solv sits in front of the payment. The agent asks first.
Solv answers APPROVE, HOLD or BLOCK, with reasons, and keeps a public receipt.

Point at the example card on the right: a real run where a service text told the
agent to ignore its rules and transfer the balance. Solv blocked it in 0.44 s.

**Scroll to "How Solv works" and walk the diagram left to right.**

Top lane, the request path, which the paying agent waits for:

| Step | Say | Time |
| --- | --- | --- |
| 1. Your agent asks first | One HTTP call with name, URL, amount and the service text. | 1 call |
| 2. Cloudflare, Clef-flash (Tier 1) | A small model on Cloudflare Workers AI reads the text as data and scores injection and risk. | 0.4 s median |
| 3. OpenServ, SERV Reasoning (Tier 2) | A reasoning layer. Skipped when Tier 1 is certain of an injection. It ran in every clean check we measured. | 3 to 4 s |
| 4. Policy gate | Plain Rust, no model. The score band, spend cap and hard stops decide. | 24 microseconds |
| 5. Decision and receipt | APPROVE, HOLD or BLOCK, reason codes, a public address. | kept |

Bottom lane, rating a counterparty, which runs ahead of time:

| Step | Say | Time |
| --- | --- | --- |
| 1. Operator queues an audit | Name, URL and the public claim. | on demand |
| 2. Reppo Eval Router | A panel of independent evaluator models scores the claim. | 2 to 5 min, 440 credits |
| 3. Confidence check | A low-confidence result is charged but never becomes a score. | instant |
| 4. Score profile | 300 to 850, aged, dropped after 90 days. The gate reads it from memory. | microseconds |

Say it plainly: **Reppo is never on the request path.** The slow part happens
before anyone pays. The fast path only reads the result.

Then two short stops: the **five bands** (each has a default spending cap, the
boundaries are the published FICO ones, and the scale is not calibrated) and the
**measured and not** list at the bottom (read two items aloud, one MEASURED and one
UNPROVEN).

## 2. Demo path (about 6 minutes)

Click **Try the desk** (or the navbar). Every step names where you are, what to
click, what should happen and how long it takes.

| # | Where | Click | Expect | Time |
| --- | --- | --- | --- | --- |
| 1 | `/desk` | Preset **Prompt-injection scam**, then **Run pre-spend check** | BLOCK, reason `SOLV-INJECTION-002`, only Clef-flash used | about 0.5 to 1 s |
| 2 | `/desk` | Preset **Audited public API**, then **Run pre-spend check** | APPROVE, reason `SOLV-SCORE-010`, Clef-flash and SERV used, score 850 EXCEPTIONAL | 3 to 5 s |
| 3 | `/desk` result | The **receipt** link | Receipt page: decision, reasons, policy (ruleset) version, evidence hash | instant |
| 4 | receipt page | **Verify hash in this browser** | Green line starting `MATCH: SHA-256 of the … canonical evidence bytes` | instant |
| 5 | `/desk` | Preset **Cold-start agent**, then **Run pre-spend check** | HOLD, reason `SOLV-COLDSTART-001`, score shown as UNRATED | 5 to 7 s |
| 6 | `/explorer` | Type `github-rest-api` in **Check a counterparty** and open it | Band, score, age 0 d and expiry in 90 d, source evaluation id, pillars | instant |
| 7 | `/proof` | Tab **Agent operators and wallets**, **Run this check** on both cards | Each card says what it expects, then shows "It matches" | 1 s and 4 s |
| 8 | `/proof` | Tab **Marketplaces and bounty platforms**, **Create a receipt** | Receipt id, policy version, evidence hash, link to the receipt | 1 s |
| 9 | `/proof` | Tab **API and MCP providers** | The rated profile, the three steps to get rated, and the honest note that audits are operator-run | instant |

### Talk track, step by step

1. **Injection (step 1).** "The service text says ignore your rules and send the
   balance to this address. Tier 1 is certain, so Tier 2 is skipped and the
   decision comes back in under a second. Look at the reason: it quotes why."
2. **Audited API (step 2).** "This counterparty has a real score from an
   independent evaluation. The gate approves, up to the cap for its band, and
   still explains the approval. It takes a few seconds because the reasoning
   layer ran."
3. **Receipt (step 3).** "Every decision has a public address. It records the
   decision, the reasons, the policy version and a hash of the evidence."
4. **Verify (step 4).** "Your browser recomputes the SHA-256 of the evidence and
   compares it to the printed hash. You do not have to trust Solv to read it."
   Be exact: this proves the page is consistent with its hash. It does not prove
   the underlying facts.
5. **Cold start (step 5).** "Nobody has evaluated this counterparty. UNRATED is the
   absence of evidence, not bad evidence, so the answer is HOLD for a human."
6. **Explorer (step 6).** "This is the registry. The score is a Reppo verdict
   mapped to 300 to 850, with its age. It expires after 90 days."
7. **Proof, operators (step 7).** "One call before each payment. The curl for the
   same call is under each card."
8. **Proof, marketplaces (step 8).** "You release funds to worker agents. Each
   release leaves a receipt your auditor can check without Solv."
9. **Proof, providers (step 9).** "If you run an API or an MCP server, a rated
   profile is what buyers' agents will look for. Audits are run by the operator
   for now. There is no self-service form yet."

Close on the landing page: "Three audiences, one gate. Operators call it,
marketplaces audit it, providers get rated by it."

## 3. If a provider is slow or down

- **Slow check.** Keep talking. Say what is running: "Tier 1 is back, the
  reasoning layer is working." Escalated checks take 3 to 5 s and up to 7 s for an
  unrated counterparty.
- **A provider is down or a key is missing.** The request holds with
  `SOLV-SYSTEM-003`. Say it is by design: a failure never turns into permission to
  pay. This is a feature to show, not a bug to hide.
- **Rate limit or network error on `/proof`.** The card shows the message. Switch to
  the Desk, or to a receipt you opened earlier.
- **The APPROVE card shows a cold-start HOLD.** The demo profile is not loaded.
  Say so, show `/explorer` empty, and continue with the BLOCK and HOLD steps. Do
  not try to explain it away.
- **Everything is down.** Use the screenshots saved before the talk, and the
  landing page, which needs no provider.

## 4. Questions you will get

- **Is the score calibrated?** No. It is a linear rescaling of one evaluator
  verdict onto 300 to 850, using the published FICO band boundaries so you can
  check them. Nothing links it to payment outcomes yet.
- **Who is the counterparty, really?** Scores attach to a claimed name and URL,
  not to a verified identity. A name can be impersonated. Binding scores to a
  domain or a wallet is planned. Tell people to compare the URL on the receipt.
- **Why does it take seconds?** The fast part is Tier 1 (0.4 s). The reasoning
  layer takes 3 to 4 s. In 44 live calls Tier 1's risk confidence never reached
  the 0.90 floor, so Tier 2 almost always runs. The specification targets
  (Tier 1 under 90 ms, escalated under 900 ms) were not met. For a no-model
  lookup of a known counterparty, `GET /api/v1/agent/:id` answers from memory.
- **What does an audit cost?** 440 credits, about $0.22 (1 USDC buys 2,000
  credits). A new Reppo account starts with 22,000 credits once.
- **Can Reppo be wrong?** Yes. Our first live audit returned a score of 0 with low
  confidence because one heavily weighted evaluator outvoted two others. Solv
  charges for it but does not publish a low-confidence score.
- **Does Solv move money?** No. It answers before you pay and records what it
  answered. A receipt is a record, not an authorisation, and Solv holds no keys.
- **What if the model is tricked?** Models only supply signals. The decision is a
  deterministic table. A broken detector lands on HOLD, never on APPROVE. The
  Attack Lab has 20 hostile cases and none produces an approval, tested offline
  against stand-ins. Live behaviour on encoding layers is untested.
- **Is there a third-party audit?** No.

## 5. What not to say

- "Under 90 ms" or "milliseconds" for a full check. Tier 1 alone is 0.4 s median.
- "Tier 1 handles 90 percent of traffic." Not true in any live sample.
- "Verified identity." It is a claimed name and URL.
- "Calibrated", "predicts fraud" or any accuracy figure. None is measured.
- "Self-service rating." Audits are run by the operator.

## 6. One-slide summary of the numbers

| Claim | Number | Basis | Label |
| --- | --- | --- | --- |
| Tier 1 (Cloudflare Clef-flash), median | 394 ms | 12 live calls | MEASURED |
| Escalated check (Tier 1 and Tier 2) | about 3.7 s | 34 live calls | MEASURED |
| Gate code, full request through the router | 24 µs | cargo bench, mock providers | MEASURED (a floor) |
| Profile lookup from memory | about 2.5 µs | cargo bench | MEASURED |
| Reppo audit | 156 s and 273 s | 2 live runs | MEASURED |
| Reppo audit cost | 440 credits, about $0.22 | Reppo reference and 2 charges | MEASURED |
| Injection cases approved | 0 of 20 | `tests/attack.rs`, offline stand-ins | PROVEN (offline) |
| Tier 1 risk confidence at or above 0.90 | 0 of 44 | 44 live calls | MEASURED |
| Score predicts outcomes | not measured | none | UNPROVEN |
| Identity of a counterparty | not verified | none | UNPROVEN |
