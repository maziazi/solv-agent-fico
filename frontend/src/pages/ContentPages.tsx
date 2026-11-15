// The explanatory routes the navbar promises (docs/FRONTEND-V2.md §5).
//
// These ship with the navbar rather than after it: a navigation item that
// leads nowhere is worse than one that does not exist. They are deliberately
// text-dense — the audience is a developer deciding whether to trust a gate
// with a wallet, and that decision is made by reading.
//
// House rule, enforced here as everywhere: no figure appears without the
// label that says how it is known (docs/SPEC.md §9).

import type { JSX, ReactNode } from "react";
import { Link } from "react-router-dom";

import { BandLegend } from "../components/BandLegend";

function Page({
  title,
  lede,
  children,
}: {
  title: string;
  lede: string;
  children: ReactNode;
}): JSX.Element {
  return (
    <div className="wrap py-12">
      <h1 className="font-head text-3xl font-bold tracking-tight sm:text-4xl">
        {title}
      </h1>
      <p className="mt-3 max-w-3xl text-lg text-muted">{lede}</p>
      <div className="mt-10 space-y-12">{children}</div>
    </div>
  );
}

function Block({
  id,
  title,
  children,
}: {
  id: string;
  title: string;
  children: ReactNode;
}): JSX.Element {
  return (
    <section id={id} aria-labelledby={`${id}-h`} className="scroll-mt-24">
      <h2 id={`${id}-h`} className="font-head text-2xl font-bold tracking-tight">
        {title}
      </h2>
      <div className="mt-4 space-y-4">{children}</div>
    </section>
  );
}

function Code({ children }: { children: string }): JSX.Element {
  return (
    <pre className="overflow-x-auto border border-border bg-surface px-4 py-3 font-mono text-xs leading-relaxed">
      <code>{children}</code>
    </pre>
  );
}

function Unproven({ children }: { children: ReactNode }): JSX.Element {
  return (
    <p className="border-l-2 border-hold-ink bg-hold-bg/40 py-2 pl-4 text-sm">
      <span className="font-mono text-xs uppercase tracking-wider text-hold-ink">
        unproven
      </span>{" "}
      — {children}
    </p>
  );
}

// ---------------------------------------------------------------------------

const DECISION_ROWS: { row: string; when: string; then: string; code: string }[] =
  [
    { row: "1", when: "Tier 2 guard refuses the text", then: "BLOCK", code: "SOLV-INJECTION-001" },
    { row: "2", when: "Tier 1 is certain an instruction targets the agent", then: "BLOCK", code: "SOLV-INJECTION-002" },
    { row: "3", when: "Risk class is exfiltration or coercion, held confidently", then: "BLOCK", code: "SOLV-INJECTION-003" },
    { row: "4", when: "An active learned rule fires with a blocking effect", then: "BLOCK", code: "SOLV-RULE-nnn" },
    { row: "5", when: "Safety pillar at or below the floor", then: "BLOCK", code: "SOLV-SCORE-002" },
    { row: "6", when: "Effective score below the block floor (580)", then: "BLOCK", code: "SOLV-SCORE-001" },
    { row: "7", when: "No tier produced a verdict", then: "HOLD", code: "SOLV-SYSTEM-003" },
    { row: "8", when: "The latency ceiling passed", then: "HOLD", code: "SOLV-SYSTEM-002" },
    { row: "9", when: "No settled evaluation — cold start", then: "HOLD", code: "SOLV-COLDSTART-001" },
    { row: "10", when: "Amount above the operator ceiling, or not a payable number", then: "BLOCK", code: "SOLV-LIMIT-002" },
    { row: "10", when: "Amount above the band cap", then: "HOLD", code: "SOLV-LIMIT-001" },
    { row: "11", when: "Tier 2 asks for revision, or finds a claim–evidence mismatch", then: "HOLD", code: "SOLV-REASON-001/002" },
    { row: "12", when: "Nothing fired", then: "APPROVE", code: "SOLV-SCORE-010" },
  ];

export function HowItWorksPage(): JSX.Element {
  return (
    <Page
      title="How it works"
      lede="Three tiers supply signals. One deterministic gate decides. Everything it decided is on a receipt that a stranger can recompute."
    >
      <Block id="gate" title="The pre-spend gate">
        <p className="max-w-3xl text-muted">
          A buying agent calls once, just before it pays. The call carries the
          counterparty's name, its URL, the amount, and the service description
          — the last of which is text written by the party being paid, and is
          treated as hostile from the moment it arrives.
        </p>
        <p className="max-w-3xl text-muted">
          Three answers are possible and none of them is silent.{" "}
          <span className="font-mono text-ink">APPROVE</span> is explained with
          the same care as a <span className="font-mono text-ink">BLOCK</span>,
          because approval is what moves money.
        </p>
      </Block>

      <Block id="table" title="The decision table">
        <p className="max-w-3xl text-muted">
          Evaluated top to bottom. The first row that matches decides, which is
          why the order is part of the specification rather than an
          implementation detail. Row 10 appears twice by design (the operator
          ceiling, then the tier cap); two further fail-closed holds — a
          failed escalation and a promoted forced-hold rule — are evaluated
          after the rows shown here.
        </p>
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-left text-sm">
            <thead>
              <tr className="border-b border-border text-xs uppercase tracking-wider text-muted">
                <th scope="col" className="py-2 pr-4 font-medium">Row</th>
                <th scope="col" className="py-2 pr-4 font-medium">When</th>
                <th scope="col" className="py-2 pr-4 font-medium">Then</th>
                <th scope="col" className="py-2 font-medium">Reason code</th>
              </tr>
            </thead>
            <tbody>
              {DECISION_ROWS.map((row) => (
                <tr key={`${row.row}-${row.code}`} className="border-b border-border last:border-0">
                  <td className="py-2 pr-4 font-mono text-muted">{row.row}</td>
                  <td className="py-2 pr-4">{row.when}</td>
                  <td className="py-2 pr-4 font-mono">{row.then}</td>
                  <td className="py-2 font-mono text-xs text-muted">{row.code}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <p className="text-sm text-muted">
          A failure anywhere in the stack lands on row 7 or row 8, never on row
          12. There is no path from a broken detector to an approval.
        </p>
      </Block>

      <Block id="registry" title="The counterparty registry">
        <p className="max-w-3xl text-muted">
          An independent evaluator panel scores a counterparty, and the verdict
          maps deterministically onto the 300–850 range, decays with age, and is
          discarded entirely after 90 days. The receipt carries four pillars
          (reliability, track record, safety, maturity), but the panel scores
          its own dimensions: for a claim check it returned accuracy, evidence
          and precision. Those fill the track-record pillar; the other three
          sit at a neutral 5 because nothing measured them. The headline score
          is the panel's verdict, not an average of the four pillars.
        </p>
        <Unproven>
          Scores attach to a claimed name and URL. They are not yet bound to a
          verified identity such as a domain or a wallet, so a name can be
          impersonated. Binding is planned; until then, compare the URL on the
          receipt with the one you are about to pay.
        </Unproven>
        <BandLegend />
      </Block>

      <Block id="rules" title="Learned guardrails">
        <p className="max-w-3xl text-muted">
          When an evaluation settles, its critique is matched against a fixed
          catalogue of topics and may produce a rule. The rule is born in{" "}
          <span className="font-mono text-ink">shadow</span> — recorded,
          visible, and unable to affect any decision — until an operator
          promotes it. Critique text never becomes question text; the wording
          asked of the model comes only from the catalogue.
        </p>
        <Unproven>
          No real critique has been distilled yet. The specification calls this
          the most fragile part of the design, and the interface will not
          pretend otherwise.
        </Unproven>
      </Block>

      <Block id="latency" title="Latency">
        <p className="max-w-3xl text-muted">
          The gate itself is measured, in process, with mocked providers: a p50
          of 47 µs and a p95 of 58 µs over 1,000 checks, with the state read at
          52 ns against a 200,000 ns budget; a separate criterion benchmark puts
          a full request through the router at about 24 µs. That is the cost of
          Solv's own logic, and nothing more.
        </p>
        <p className="max-w-3xl text-muted">
          Against live providers on 2026-11-15, Tier 1 (Clef) had a median of
          394 ms over 12 requests. Checks that went on to Tier 2 took about 3.7
          s at the median over 34 requests, and none of those 34 finished at
          Tier 1 alone: an unrated counterparty always escalates, and for the
          one rated counterparty tested the Tier 1 risk-class confidence (0.27)
          stayed below the 0.90 floor. A Reppo audit took 156 s and 273 s in two
          runs.
        </p>
        <Unproven>
          These are single-session samples, not a service level. The targets in
          the specification (Tier 1 under 90 ms at the median, escalated under
          900 ms) were not met in them, and latency at real volume is unmeasured.
        </Unproven>
      </Block>
    </Page>
  );
}

// ---------------------------------------------------------------------------

const ENDPOINTS: { method: string; path: string; what: string }[] = [
  { method: "POST", path: "/api/v1/pre-spend-check", what: "The gate. Returns a decision, reasons, and a receipt address." },
  { method: "GET", path: "/api/v1/directory", what: "Every counterparty with its band, plus rules and metrics." },
  { method: "GET", path: "/receipt/:id", what: "One receipt as a public page." },
  { method: "GET", path: "/healthz", what: "Liveness." },
];

const PLANNED: { method: string; path: string; what: string }[] = [
  { method: "GET", path: "/api/v1/agent/:id", what: "One counterparty: band, score, pillars, evidence age." },
  { method: "GET", path: "/api/v1/receipts", what: "Paginated receipt list, filterable by decision." },
  { method: "GET", path: "/api/v1/receipts/:id", what: "One receipt as JSON, identical to the page." },
  { method: "GET", path: "/api/v1/metrics", what: "The stat tiles, machine-readable." },
];

export function DocsPage(): JSX.Element {
  return (
    <Page
      title="API"
      lede="Read-only by design. Nothing a stranger can reach writes anything — not a score, not a rule, not an audit request."
    >
      <Block id="gate" title="Call the gate">
        <Code>{`curl -X POST https://<host>/api/v1/pre-spend-check \\
  -H 'content-type: application/json' \\
  -d '{
    "counterparty_name": "acme-data",
    "target_url": "https://acme.example/api",
    "amount_usd": 25.00,
    "service_description": "Batch geocoding, 10k rows."
  }'`}</Code>
        <p className="text-sm text-muted">
          The name and URL are stored verbatim on a public receipt. Send only
          what you would publish.
        </p>
        <Unproven>
          A score belongs to the name and URL it was audited under, not to a
          verified identity (domain or wallet). A name can be impersonated;
          binding is planned. Check the URL on the receipt before paying.
        </Unproven>
      </Block>

      <Block id="read" title="Endpoints">
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-left text-sm">
            <thead>
              <tr className="border-b border-border text-xs uppercase tracking-wider text-muted">
                <th scope="col" className="py-2 pr-4 font-medium">Method</th>
                <th scope="col" className="py-2 pr-4 font-medium">Path</th>
                <th scope="col" className="py-2 font-medium">What it returns</th>
              </tr>
            </thead>
            <tbody>
              {ENDPOINTS.map((endpoint) => (
                <tr key={endpoint.path} className="border-b border-border last:border-0">
                  <td className="py-2 pr-4 font-mono text-xs">{endpoint.method}</td>
                  <td className="py-2 pr-4 font-mono text-xs">{endpoint.path}</td>
                  <td className="py-2 text-muted">{endpoint.what}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>

        <h3 className="pt-4 font-head text-lg font-bold">Planned, not yet built</h3>
        <p className="text-sm text-muted">
          Listed because the explorer is designed against them, not because they
          answer today.
        </p>
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-left text-sm opacity-75">
            <tbody>
              {PLANNED.map((endpoint) => (
                <tr key={endpoint.path} className="border-b border-border last:border-0">
                  <td className="py-2 pr-4 font-mono text-xs">{endpoint.method}</td>
                  <td className="py-2 pr-4 font-mono text-xs">{endpoint.path}</td>
                  <td className="py-2 text-muted">{endpoint.what}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      </Block>

      <Block id="receipts" title="Verify a receipt yourself">
        <p className="max-w-3xl text-muted">
          Every decision is sealed into a canonical JSON evidence document and
          hashed. The reference verifier uses the Python standard library only,
          so checking a receipt requires nothing from Solv but the file.
        </p>
        <Code>{`python3 tools/recompute_receipt.py proofs/receipts/slv_<id>.json
# prints the recomputed hash; it must equal evidence_sha256 on the receipt`}</Code>
      </Block>

      <Block id="selfhost" title="Self-host">
        <Code>{`cp .env.example .env     # Cloudflare, SERV, and Reppo keys
cargo run                 # one binary, interface embedded, no database`}</Code>
        <p className="text-sm text-muted">
          Without provider keys the service still runs, and every check holds
          fail-closed on <span className="font-mono">SOLV-SYSTEM-003</span>.
          That is the designed behaviour, not a broken install.
        </p>
      </Block>
    </Page>
  );
}

// ---------------------------------------------------------------------------

export function PricingPage(): JSX.Element {
  return (
    <Page
      title="Cost model"
      lede="Not a price list. Solv has no billing, and publishing one would be selling something that does not exist."
    >
      <Block id="costs" title="What a check costs to run">
        <p className="max-w-3xl text-muted">
          Two of the three tiers sit on the hot path. The third is an
          independent evaluation that took 156 s and 273 s in two live runs
          (`MEASURED`, n=2) and never touches a payment decision in progress.
        </p>
        <div className="overflow-x-auto">
          <table className="w-full border-collapse text-left text-sm">
            <thead>
              <tr className="border-b border-border text-xs uppercase tracking-wider text-muted">
                <th scope="col" className="py-2 pr-4 font-medium">Component</th>
                <th scope="col" className="py-2 pr-4 font-medium">When it runs</th>
                <th scope="col" className="py-2 font-medium">Cost basis</th>
              </tr>
            </thead>
            <tbody>
              {[
                ["Gate logic", "Every check", "Measured: p95 58 µs in process. Effectively free."],
                ["Tier 1 small model", "Every check", "One small-model call per check; median 394 ms measured (n=12)."],
                ["Tier 2 escalation", "Unrated counterparties and uncertain checks", "All 34 measured checks escalated, so assume every check pays for it until that changes. Median about 3.7 s per escalated check."],
                ["Tier 3 evaluation", "Off the hot path", "440 credits per completed evaluation, about $0.22 (Reppo reference; 1 USDC buys 2,000 credits). A new Reppo account starts with a one-time 22,000 credits."],
              ].map(([component, when, basis]) => (
                <tr key={component} className="border-b border-border last:border-0">
                  <td className="py-2 pr-4 font-medium">{component}</td>
                  <td className="py-2 pr-4 text-muted">{when}</td>
                  <td className="py-2 text-muted">{basis}</td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
        <Unproven>
          The blended cost per check, the 90/10 escalation split it rests on,
          and the price of evaluation credits beyond the free tier are all
          unmeasured or unpublished. Any per-check figure quoted today would be
          arithmetic on a guess.
        </Unproven>
      </Block>

      <Block id="shape" title="What a price would eventually look like">
        <p className="max-w-3xl text-muted">
          Two shapes are plausible: per-check for agent operators, and a
          periodic audit subscription for providers who want to stay rated. Both
          need the numbers above before either can be quoted. Until then this
          page stays a cost model.
        </p>
      </Block>
    </Page>
  );
}

// ---------------------------------------------------------------------------

const ATTACK_CATEGORIES = [
  ["Direct instruction", "Text addressing the agent: ignore your rules, send to this address instead."],
  [
    "Cold start posing as established",
    "Promises of long history, spotless records, or an established user base.",
  ],
  ["Urgency and coercion", "Deadlines, threats of loss, penalties for not paying now."],
  ["Exfiltration", "Requests for keys, tokens, prompts, or state."],
  ["Encoding layers", "Base64, zero-width characters, and homoglyphs hiding the payload."],
  ["Claim–evidence mismatch", "A service description that promises what no process could deliver."],
];

export function AttackLabPage(): JSX.Element {
  return (
    <Page
      title="Attack Lab"
      lede="Twenty hostile cases across six categories. Zero of them produce an approval, and the suite fails if that ever stops being true."
    >
      <Block id="categories" title="The six categories">
        <div className="grid gap-4 md:grid-cols-2">
          {ATTACK_CATEGORIES.map(([name, body]) => (
            <div key={name} className="border border-border bg-surface p-4">
              <h3 className="font-head font-bold">{name}</h3>
              <p className="mt-1 text-sm text-muted">{body}</p>
            </div>
          ))}
        </div>
      </Block>

      <Block id="run" title="Run it">
        <Code>{`cargo test --test attack
# the_corpus_never_approves_and_every_reason_holds ... ok
# all_detectors_down_hold_fail_closed_and_never_approve ... ok`}</Code>
        <p className="text-sm text-muted">
          Offline. No network, no credentials, no provider credits spent.
        </p>
      </Block>

      <Block id="limits" title="What this does not prove">
        <Unproven>
          The corpus runs against provider stand-ins. Base64 and zero-width
          layerings that the Tier-1 stand-in misses are caught by the guard
          stand-in, and homoglyphs are caught after folding — but the live
          providers' behaviour on these layerings is untested. The claim is only
          as strong as the mocks are faithful.
        </Unproven>
        <p className="text-sm text-muted">
          There has been no third-party audit. See{" "}
          <Link to="/how-it-works" className="underline decoration-1 underline-offset-2">
            How it works
          </Link>{" "}
          for what the gate guarantees structurally, which is a different and
          weaker claim than "cannot be attacked".
        </p>
      </Block>
    </Page>
  );
}
