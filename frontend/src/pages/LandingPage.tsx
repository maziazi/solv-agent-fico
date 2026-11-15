// Landing page (docs/FRONTEND-V2.md §3).
//
// Structure is borrowed from stripe.com/radar: lifecycle framing, grouped
// capability blocks, an evidence row, numbered steps, integration options,
// audience tabs, FAQ, closing CTA. Palette and typography stay Warm Ledger
// (docs/DESIGN.md §1–§3).
//
// One deliberate departure from the reference: Stripe prints large outcome
// numbers with nothing behind them on the page itself. Every number here
// carries the label that says how it is known, and a claim without evidence
// is written as a sentence rather than dressed up as a statistic
// (docs/SPEC.md §9).

import { useState } from "react";
import type { JSX, ReactNode } from "react";
import { Link } from "react-router-dom";

import { BandLegend } from "../components/BandLegend";

type Verification = "PROVEN" | "MEASURED" | "UNPROVEN";

const VERIFICATION_CLASS: Record<Verification, string> = {
  PROVEN: "border-approve-ink text-approve-ink",
  MEASURED: "border-approve-ink text-approve-ink",
  UNPROVEN: "border-hold-ink text-hold-ink",
};

function ClaimLabel({ level }: { level: Verification }): JSX.Element {
  return (
    <span
      className={`inline-block border px-1.5 py-px font-mono text-[0.65rem] uppercase tracking-wider ${VERIFICATION_CLASS[level]}`}
    >
      {level}
    </span>
  );
}

function Section({
  id,
  title,
  lede,
  children,
}: {
  id: string;
  title: string;
  lede?: string;
  children: ReactNode;
}): JSX.Element {
  return (
    <section
      id={id}
      aria-labelledby={`${id}-title`}
      className="border-t border-border py-14"
    >
      <h2
        id={`${id}-title`}
        className="font-head text-2xl font-bold tracking-tight sm:text-3xl"
      >
        {title}
      </h2>
      {lede && <p className="mt-2 max-w-2xl text-muted">{lede}</p>}
      <div className="mt-8">{children}</div>
    </section>
  );
}

const CURL = `curl -X POST https://<host>/api/v1/pre-spend-check \\
  -H 'content-type: application/json' \\
  -d '{
    "counterparty_name": "acme-data",
    "target_url": "https://acme.example/api",
    "amount_usd": 25.00,
    "service_description": "<text written by the counterparty>"
  }'

{
  "decision": "BLOCK",
  "reasons": [{ "code": "SOLV-INJECTION-002", "severity": "critical" }],
  "tier": "POOR",
  "max_safe_spend": 0.00,
  "evidence_sha256": "c53c9238…",
  "receipt_url": "/receipt/slv_006af989…"
}`;

function Hero(): JSX.Element {
  return (
    <section aria-labelledby="hero-title" className="py-14 sm:py-20">
      <div className="grid gap-10 lg:grid-cols-[1fr_1.1fr] lg:items-start">
        <div>
          <p className="font-mono text-xs uppercase tracking-[0.2em] text-accent-fill">
            Pre-spend gate &amp; counterparty registry
          </p>
          <h1
            id="hero-title"
            className="mt-3 font-head text-4xl font-bold leading-[1.1] tracking-tight sm:text-5xl"
          >
            The credit check an AI agent runs before it pays.
          </h1>
          <p className="mt-5 max-w-xl text-lg text-muted">
            An agent about to send money cannot tell a real counterparty from a
            fabricated one, and it will read instructions hidden in a service
            description as if they came from you. Solv answers{" "}
            <span className="font-mono text-ink">APPROVE</span>,{" "}
            <span className="font-mono text-ink">HOLD</span>, or{" "}
            <span className="font-mono text-ink">BLOCK</span> — with the reasons
            attached, on a receipt anyone can verify.
          </p>
          <div className="mt-7 flex flex-wrap gap-3">
            <Link
              to="/desk"
              className="border border-accent-fill bg-accent-fill px-4 py-2.5 font-head font-medium tracking-wide text-bg hover:opacity-90"
            >
              Try the desk
            </Link>
            <Link
              to="/explorer"
              className="border border-ink px-4 py-2.5 font-head font-medium tracking-wide hover:bg-surface"
            >
              Check a counterparty
            </Link>
          </div>
          <p className="mt-4 text-sm text-muted">
            Solv records decisions. It never moves money and holds no keys.
          </p>
        </div>

        <div className="border border-border bg-surface">
          <div className="flex items-center justify-between border-b border-border px-4 py-2">
            <span className="font-mono text-xs text-muted">
              one call, before the payment
            </span>
            <span className="font-mono text-xs text-muted">POST</span>
          </div>
          <pre className="overflow-x-auto px-4 py-4 font-mono text-xs leading-relaxed">
            <code>{CURL}</code>
          </pre>
        </div>
      </div>
    </section>
  );
}

const LIFECYCLE = [
  {
    stage: "Before the first payment",
    wrong:
      "The counterparty does not exist, the URL is dead, or the claims have nothing behind them.",
    does: "An independent evaluation produces a settled score. No evaluation means UNRATED — not a guess, and not a middling default.",
  },
  {
    stage: "At the moment of payment",
    wrong:
      "Instructions aimed at your agent are buried in the service description, or the amount is far above what this counterparty has earned.",
    does: "A small-model check, escalation to a reasoning layer when the first is unsure or the counterparty is unrated, then a deterministic gate. If a detector is down, the request holds.",
  },
  {
    stage: "After the payment",
    wrong:
      "Evidence goes stale, and a pattern nobody wrote a rule for keeps working.",
    does: "Scores decay with age and are discarded at 90 days. Critiques from evaluations become shadow rules an operator can promote.",
  },
];

const CAPABILITIES = [
  {
    title: "Pre-spend gate",
    body: "Three answers, and never one of them silent. Every decision carries reason codes, the evidence each rested on, and the latency each tier spent.",
    to: "/how-it-works#gate",
    cta: "See the decision table",
  },
  {
    title: "Counterparty registry",
    body: "Five bands on the published FICO boundaries, each with its own spending cap. Scores age and expire. No actor — not even the operator — can write one.",
    to: "/explorer",
    cta: "Open the directory",
  },
  {
    title: "Learned guardrails",
    body: "An evaluator's critique is distilled against a fixed catalogue into a shadow rule. It enforces only after a human promotes it.",
    to: "/how-it-works#rules",
    cta: "How a rule is born",
  },
  {
    title: "Public receipts",
    body: "Every decision is sealed into a hashed evidence document. A third party recomputes the hash with a Python script that has no dependencies.",
    to: "/docs#receipts",
    cta: "Verify one yourself",
  },
];

const EVIDENCE: {
  value: string;
  label: string;
  level: Verification;
  source: string;
}[] = [
  {
    value: "20 / 20",
    label: "injection cases refused",
    level: "PROVEN",
    source: "tests/attack.rs — zero approvals across six attack categories",
  },
  {
    value: "0",
    label: "endpoints that set a score",
    level: "PROVEN",
    source: "No endpoint sets a score. The audit worker writes only what a settled evaluation says.",
  },
  {
    value: "recomputable",
    label: "receipt hash, by a third party",
    level: "PROVEN",
    source: "tools/recompute_receipt.py — Python standard library only",
  },
  {
    value: "fail-closed",
    label: "when every tier is down",
    level: "PROVEN",
    source: "A system failure never becomes permission to pay.",
  },
];

const STEPS = [
  {
    n: "01",
    title: "State lookup",
    body: "The counterparty's settled score and the active rule set, read in process.",
  },
  {
    n: "02",
    title: "Tier 1 — small model",
    body: "Closed questions to a small model. Counterparty text arrives as data, never in a prompt position. Measured median in live runs: 394 ms.",
  },
  {
    n: "03",
    title: "Tier 2 — escalation",
    body: "Bounded reasoning and a prompt guard. Consulted when Tier 1 is unsure or the counterparty is unrated; in every live run so far it was consulted, and it adds about 3 seconds.",
  },
  {
    n: "04",
    title: "Policy gate",
    body: "Twelve ordered rows in Rust. Models supply signals; this step decides, and the same inputs always give the same answer.",
  },
  {
    n: "05",
    title: "Receipt",
    body: "Decision, reasons, evidence, and the hash — at a permanent public address.",
  },
];

const SEGMENTS = [
  {
    id: "operators",
    label: "Agent operators & wallets",
    body: "You give an agent a budget and need the spend bounded by something other than hope. Solv is one binary with no database, it fails closed when a provider is down, and the cap it applies is the cap printed on the receipt.",
    points: [
      "Self-host, or call the hosted gate",
      "Per-band caps plus an absolute ceiling nothing overrides",
      "Reasons on every decision, including approvals",
    ],
    cta: { label: "Try the desk", to: "/desk" },
  },
  {
    id: "platforms",
    label: "Marketplaces & bounty platforms",
    body: "You release funds to worker agents and have to show, later, why each release was allowed. Every decision is a receipt with a policy version attached, so an audit reconstructs the rules that were in force at the time.",
    points: [
      "Receipt ledger with a stable address per decision",
      "Policy version recorded on every decision",
      "Read API for scoring inside your own system",
    ],
    cta: { label: "Read the API docs", to: "/docs" },
  },
  {
    id: "providers",
    label: "API & MCP providers",
    body: "You are legitimate and want buying agents to see it. A settled evaluation gives you a band and a page you can hand to a buyer — scored by an independent evaluator, not by Solv and not by you.",
    points: [
      "Four pillars on the receipt; today only track record is filled from the evaluator, the other three sit at a neutral 5",
      "A shareable counterparty page",
      "Scores expire, so staying rated means staying evidenced",
    ],
    cta: { label: "How to get rated", to: "/explorer" },
  },
];

const FAQ = [
  {
    q: "Is the score calibrated?",
    a: "No. The 300–850 scale is a linear rescaling of one evaluator verdict, and the band boundaries are the published FICO boundaries so you can check them against a public reference. Solv has no outcome data — no counterparty has been approved and then observed to fail — so no false-positive or false-negative rate exists. This is labelled UNPROVEN wherever it appears.",
  },
  {
    q: "What happens when a provider is down?",
    a: "The request holds. A system failure never becomes permission to pay, and the receipt names the tier that was unreachable instead of quietly producing a verdict.",
  },
  {
    q: "Who can change a score?",
    a: "Nobody. There is no endpoint that writes one. A score is written only by the worker that reads a settled evaluation, and it writes only what that evaluation says.",
  },
  {
    q: "What does UNRATED mean?",
    a: "Nobody has evaluated this counterparty. That is an absence of evidence, not weak evidence, so there is no number and no amount is approved — the request holds for a human.",
  },
  {
    q: "Does Solv move money?",
    a: "No. It answers before you pay and records what it answered. A receipt is a record, not an authorisation, and Solv holds no keys.",
  },
  {
    q: "Can someone claim another counterparty's name?",
    a: "Today, yes. A score attaches to a claimed name and URL; it is not yet bound to a verified identity such as a domain or a wallet, so a name can be impersonated. Binding scores to verifiable identities is planned. Until then, check that the URL on a receipt is the one you are about to pay.",
  },
  {
    q: "How fast is it, measured?",
    a: "Solv's own logic takes about 24 µs per request (benchmark, mocked providers). Against live providers on 2026-11-15, Tier 1 had a median of 394 ms (12 requests), and checks that escalated to Tier 2 took about 3.7 s at the median (34 requests); none of those finished at Tier 1 alone. The two Reppo audits ran 156 s and 273 s and charged 440 credits each, about $0.22. These are small samples, not a service level.",
  },
  {
    q: "What is not proven yet?",
    a: "Latency at any real volume, the quality of distilling critiques into rules, live-provider behaviour on encoding-layered injection, whether Tier 1 can ever settle a check on its own, and whether anyone wants this. The specification keeps an explicit list rather than burying it.",
  },
];

function SegmentTabs(): JSX.Element {
  const [active, setActive] = useState(SEGMENTS[0].id);
  const current = SEGMENTS.find((segment) => segment.id === active) ?? SEGMENTS[0];
  return (
    <div>
      <div role="tablist" aria-label="audiences" className="flex flex-wrap gap-2">
        {SEGMENTS.map((segment) => (
          <button
            key={segment.id}
            type="button"
            role="tab"
            id={`tab-${segment.id}`}
            aria-selected={segment.id === active}
            aria-controls={`panel-${segment.id}`}
            onClick={() => setActive(segment.id)}
            className={`border px-3 py-2 font-head text-sm font-medium ${
              segment.id === active
                ? "border-ink bg-ink text-bg"
                : "border-border bg-surface text-muted hover:text-ink"
            }`}
          >
            {segment.label}
          </button>
        ))}
      </div>
      <div
        role="tabpanel"
        id={`panel-${current.id}`}
        aria-labelledby={`tab-${current.id}`}
        className="mt-5 border border-border bg-surface p-6"
      >
        <p className="max-w-3xl">{current.body}</p>
        <ul className="mt-4 space-y-1.5">
          {current.points.map((point) => (
            <li key={point} className="flex gap-2 text-sm text-muted">
              <span aria-hidden className="text-accent">
                —
              </span>
              {point}
            </li>
          ))}
        </ul>
        <Link
          to={current.cta.to}
          className="mt-5 inline-block border border-ink px-3 py-1.5 font-head text-sm font-medium hover:bg-bg"
        >
          {current.cta.label}
        </Link>
      </div>
    </div>
  );
}

export function LandingPage(): JSX.Element {
  return (
    <div className="mx-auto max-w-[76rem] px-4 sm:px-6">
      <Hero />

      <Section
        id="lifecycle"
        title="Risk is not only at the moment of payment"
        lede="An agent's exposure starts before the first transfer and continues after it clears. Solv covers all three stages, and says which one a decision came from."
      >
        <div className="grid gap-5 md:grid-cols-3">
          {LIFECYCLE.map((stage) => (
            <div key={stage.stage} className="border border-border bg-surface p-5">
              <h3 className="font-head text-lg font-bold">{stage.stage}</h3>
              <p className="mt-3 text-xs uppercase tracking-wider text-muted">
                What goes wrong
              </p>
              <p className="mt-1 text-sm">{stage.wrong}</p>
              <p className="mt-4 text-xs uppercase tracking-wider text-muted">
                What Solv does
              </p>
              <p className="mt-1 text-sm">{stage.does}</p>
            </div>
          ))}
        </div>
      </Section>

      <Section
        id="capabilities"
        title="One gate, four parts"
        lede="They share a binary, a state file, and a rule that nothing is shown with more certainty than the evidence behind it."
      >
        <div className="grid gap-5 md:grid-cols-2">
          {CAPABILITIES.map((capability) => (
            <div
              key={capability.title}
              className="flex flex-col border border-border bg-surface p-6"
            >
              <h3 className="font-head text-xl font-bold">{capability.title}</h3>
              <p className="mt-2 grow text-muted">{capability.body}</p>
              <Link
                to={capability.to}
                className="mt-4 font-mono text-sm underline decoration-1 underline-offset-4"
              >
                {capability.cta} →
              </Link>
            </div>
          ))}
        </div>
      </Section>

      <Section
        id="evidence"
        title="What is actually proven"
        lede="Each of these fails a named automated test if it stops being true. Numbers that have not been measured are not on this page."
      >
        <div className="grid gap-px border border-border bg-border sm:grid-cols-2 lg:grid-cols-4">
          {EVIDENCE.map((item) => (
            <div key={item.label} className="bg-surface p-5">
              <p className="font-mono text-2xl font-medium tracking-tight tabular-nums">
                {item.value}
              </p>
              <p className="mt-1 text-sm">{item.label}</p>
              <p className="mt-3">
                <ClaimLabel level={item.level} />
              </p>
              <p className="mt-2 text-xs text-muted">{item.source}</p>
            </div>
          ))}
        </div>
        <p className="mt-4 text-sm text-muted">
          Hot-path latency is measured gate-side and published with its method
          and sample size on{" "}
          <Link
            to="/how-it-works#latency"
            className="underline decoration-1 underline-offset-2"
          >
            How it works
          </Link>
          . Live-provider latency is <ClaimLabel level="MEASURED" /> on small
          samples only: Tier 1 median 394 ms, escalated checks about 3.7 s.
        </p>
      </Section>

      <Section
        id="steps"
        title="One check, five steps"
        lede="Models supply signals. Rust decides. That split is the whole design."
      >
        <ol className="border border-border bg-surface">
          {STEPS.map((step) => (
            <li
              key={step.n}
              className="flex flex-col gap-1 border-b border-border px-5 py-4 last:border-0 sm:flex-row sm:items-baseline sm:gap-6"
            >
              <span className="font-mono text-sm text-accent-fill sm:w-10">
                {step.n}
              </span>
              <span className="font-head text-lg font-medium sm:w-56">
                {step.title}
              </span>
              <span className="text-sm text-muted sm:flex-1">{step.body}</span>
            </li>
          ))}
        </ol>
        <p className="mt-4 text-sm text-muted">
          The twelve rows the policy gate evaluates, in binding order, are on{" "}
          <Link
            to="/how-it-works#table"
            className="underline decoration-1 underline-offset-2"
          >
            How it works
          </Link>
          .
        </p>
      </Section>

      <Section
        id="bands"
        title="What a band actually allows"
        lede="A score is only useful if it changes what happens. Each band carries a spending cap the gate enforces."
      >
        <BandLegend />
      </Section>

      <Section
        id="integrate"
        title="Three ways to use it"
        lede="Call the gate, read the registry, or run the whole thing yourself."
      >
        <div className="grid gap-5 md:grid-cols-3">
          {[
            {
              title: "Call the gate",
              body: "One POST before the payment. Returns a decision, its reasons, and a receipt address.",
              to: "/docs#gate",
            },
            {
              title: "Read the registry",
              body: "Pull a counterparty's band and pillars into your own scoring, with the policy version attached.",
              to: "/docs#read",
            },
            {
              title: "Self-host",
              body: "One Rust binary with the interface embedded. No database. Provider keys stay yours.",
              to: "/docs#selfhost",
            },
          ].map((option) => (
            <div key={option.title} className="border border-border bg-surface p-5">
              <h3 className="font-head text-lg font-bold">{option.title}</h3>
              <p className="mt-2 text-sm text-muted">{option.body}</p>
              <Link
                to={option.to}
                className="mt-3 inline-block font-mono text-sm underline decoration-1 underline-offset-4"
              >
                docs →
              </Link>
            </div>
          ))}
        </div>
      </Section>

      <Section
        id="audiences"
        title="Who this is for"
        lede="Three ways to arrive at Solv, three different first steps."
      >
        <SegmentTabs />
      </Section>

      <Section id="faq" title="Frequently asked questions">
        <dl className="border border-border bg-surface">
          {FAQ.map((entry) => (
            <div key={entry.q} className="border-b border-border p-5 last:border-0">
              <dt className="font-head text-lg font-medium">{entry.q}</dt>
              <dd className="mt-1.5 max-w-3xl text-muted">{entry.a}</dd>
            </div>
          ))}
        </dl>
      </Section>

      <section
        aria-labelledby="cta-title"
        className="my-14 border border-ink bg-surface p-8 text-center"
      >
        <h2 id="cta-title" className="font-head text-2xl font-bold tracking-tight">
          Run a check and read the receipt.
        </h2>
        <p className="mx-auto mt-2 max-w-xl text-muted">
          The desk makes the same call an agent would, against the same gate,
          and the receipt it produces is public and verifiable.
        </p>
        <div className="mt-6 flex flex-wrap justify-center gap-3">
          <Link
            to="/desk"
            className="border border-accent-fill bg-accent-fill px-4 py-2.5 font-head font-medium tracking-wide text-bg hover:opacity-90"
          >
            Try the desk
          </Link>
          <a
            href="https://github.com/maziazi/solv-agent-fico"
            className="border border-ink px-4 py-2.5 font-head font-medium tracking-wide hover:bg-bg"
          >
            Read the specification ↗
          </a>
        </div>
      </section>
    </div>
  );
}
