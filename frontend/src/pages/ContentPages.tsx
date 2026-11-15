// The explanatory routes the navbar promises (docs/FRONTEND-V2.md §5).
//
// They ship with the navbar because a nav item that leads nowhere is worse
// than none. The copy is short on purpose: say what it does and give the
// number, with the label that says how the number is known (docs/SPEC.md §9).

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
    <div className="wrap py-14">
      <h1 className="text-4xl font-bold sm:text-5xl">{title}</h1>
      <p className="mt-3 max-w-2xl text-lg text-muted">{lede}</p>
      <div className="mt-12 space-y-14">{children}</div>
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
      <h2 id={`${id}-h`} className="text-2xl font-bold">
        {title}
      </h2>
      <div className="mt-4 space-y-4">{children}</div>
    </section>
  );
}

function Code({ children }: { children: string }): JSX.Element {
  return (
    <pre className="overflow-x-auto rounded-xl bg-surface px-4 py-3 font-mono text-xs leading-relaxed">
      <code>{children}</code>
    </pre>
  );
}

function Unproven({ children }: { children: ReactNode }): JSX.Element {
  return (
    <p className="flex gap-3 rounded-xl bg-hold-solid p-4 text-sm text-ink">
      <span className="h-fit rounded-full bg-hold-solid px-2 py-0.5 font-mono text-[0.7rem] font-semibold text-ink">
        UNPROVEN
      </span>
      <span>{children}</span>
    </p>
  );
}

function DataTable({
  head,
  rows,
}: {
  head: string[];
  rows: ReactNode[][];
}): JSX.Element {
  return (
    <div className="overflow-x-auto rounded-xl bg-surface">
      <table className="w-full border-collapse text-left text-sm">
        <thead>
          <tr className="bg-wash text-xs uppercase tracking-wider text-muted">
            {head.map((label) => (
              <th key={label} scope="col" className="px-4 py-2.5 font-medium">
                {label}
              </th>
            ))}
          </tr>
        </thead>
        <tbody>
          {rows.map((row, index) => (
            <tr key={index} className="">
              {row.map((cell, column) => (
                <td key={column} className="px-4 py-2.5 align-top">
                  {cell}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}

const mono = (text: string): JSX.Element => (
  <span className="font-mono text-xs">{text}</span>
);

// ---------------------------------------------------------------------------

const DECISION_ROWS: [string, string, string, string][] = [
  ["1", "Tier 2 guard refuses the text", "BLOCK", "SOLV-INJECTION-001"],
  ["2", "Tier 1 is certain an instruction targets the agent", "BLOCK", "SOLV-INJECTION-002"],
  ["3", "Risk is exfiltration or coercion, held confidently", "BLOCK", "SOLV-INJECTION-003"],
  ["4", "A promoted learned rule fires with a blocking effect", "BLOCK", "SOLV-RULE-nnn"],
  ["5", "Safety pillar at or below the floor", "BLOCK", "SOLV-SCORE-002"],
  ["6", "Score below the block floor (580)", "BLOCK", "SOLV-SCORE-001"],
  ["7", "No tier produced a verdict", "HOLD", "SOLV-SYSTEM-003"],
  ["8", "The latency ceiling passed", "HOLD", "SOLV-SYSTEM-002"],
  ["9", "No settled evaluation (cold start)", "HOLD", "SOLV-COLDSTART-001"],
  ["10", "Amount above the operator ceiling, or not a payable number", "BLOCK", "SOLV-LIMIT-002"],
  ["10", "Amount above the band cap", "HOLD", "SOLV-LIMIT-001"],
  ["11", "Tier 2 asks for revision or finds a claim and evidence mismatch", "HOLD", "SOLV-REASON-001/002"],
  ["12", "Nothing fired", "APPROVE", "SOLV-SCORE-010"],
];

export function HowItWorksPage(): JSX.Element {
  return (
    <Page
      title="How it works"
      lede="Models supply signals. One deterministic gate decides. Every decision is on a receipt anyone can recompute."
    >
      <Block id="gate" title="The pre-spend gate">
        <p className="max-w-2xl text-muted">
          A buying agent calls once, just before it pays. Solv treats the service
          text as hostile, because the party being paid wrote it.
        </p>
        <p className="text-sm text-muted">
          The gate is tried against 20 injection cases in the{" "}
          <Link to="/attack-lab" className="text-accent-ink underline underline-offset-2">
            Attack Lab
          </Link>
          .
        </p>
      </Block>

      <Block id="table" title="The decision table">
        <p className="max-w-2xl text-muted">
          The first matching row decides. Two more fail-closed holds run after
          these rows.
        </p>
        <DataTable
          head={["Row", "When", "Then", "Reason code"]}
          rows={DECISION_ROWS.map(([row, when, then, code]) => [
            <span key="r" className="font-mono text-muted">{row}</span>,
            when,
            mono(then),
            <span key="c" className="font-mono text-xs text-muted">{code}</span>,
          ])}
        />
        <p className="text-sm text-muted">
          A broken detector lands on row 7 or 8, never on row 12.
        </p>
      </Block>

      <Block id="registry" title="The counterparty registry">
        <p className="max-w-2xl text-muted">
          A panel of evaluators scores a counterparty. The verdict maps to
          300–850, decays with age and is dropped after 90 days. For a claim
          check the panel returned accuracy, evidence and precision, which fill
          the track-record pillar. The other three pillars stay at a neutral 5.
        </p>
        <Unproven>
          Scores attach to a claimed name and URL, not to a verified identity.
          A name can be impersonated. Compare the URL on the receipt with the one
          you are about to pay.
        </Unproven>
        <BandLegend />
      </Block>

      <Block id="rules" title="Learned guardrails">
        <p className="max-w-2xl text-muted">
          A settled critique can produce a rule. It starts in{" "}
          <span className="font-mono text-ink">shadow</span> and affects nothing
          until an operator promotes it.
        </p>
        <Unproven>
          No real critique has been distilled yet. The specification calls this
          the most fragile part of the design.
        </Unproven>
      </Block>

      <Block id="latency" title="Latency">
        <p className="max-w-2xl text-muted">
          Solv's own logic takes 24 µs with mocked providers. On live providers
          (2026-11-15) Tier 1 had a 394 ms median (12 requests) and escalated
          checks took about 3.7 s (34 requests). A Reppo audit took 156 s and
          273 s.
        </p>
        <Unproven>
          Single-session samples. The specification targets (Tier 1 under 90 ms,
          escalated under 900 ms) were not met.
        </Unproven>
      </Block>
    </Page>
  );
}

// ---------------------------------------------------------------------------

const ENDPOINTS: [string, string, string][] = [
  ["POST", "/api/v1/pre-spend-check", "The gate: a decision, reasons and a receipt address."],
  ["GET", "/api/v1/agent/:id", "One counterparty from memory, with no model call."],
  ["GET", "/api/v1/directory", "Every counterparty with its band, plus rules and metrics."],
  ["GET", "/receipt/:id", "One receipt as a public page."],
  ["GET", "/healthz", "Liveness."],
];

const PLANNED: [string, string, string][] = [
  ["GET", "/api/v1/receipts", "Paginated receipt list, filterable by decision."],
  ["GET", "/api/v1/receipts/:id", "One receipt as JSON, identical to the page."],
  ["GET", "/api/v1/metrics", "The stat tiles, machine-readable."],
];

export function DocsPage(): JSX.Element {
  return (
    <Page
      title="API"
      lede="No public endpoint writes a score or a rule. Each check writes one receipt."
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
          The name and URL are stored on a public receipt. Send only what you
          would publish.
        </p>
        <Unproven>
          A score belongs to the name and URL it was audited under, not to a
          verified identity. Check the URL on the receipt before paying.
        </Unproven>
      </Block>

      <Block id="read" title="Endpoints">
        <DataTable
          head={["Method", "Path", "Returns"]}
          rows={ENDPOINTS.map(([method, path, what]) => [
            mono(method),
            mono(path),
            <span key="w" className="text-muted">{what}</span>,
          ])}
        />
        <h3 className="pt-2 text-lg font-semibold">Planned, not built</h3>
        <DataTable
          head={["Method", "Path", "Returns"]}
          rows={PLANNED.map(([method, path, what]) => [
            mono(method),
            mono(path),
            <span key="w" className="text-muted">{what}</span>,
          ])}
        />
      </Block>

      <Block id="receipts" title="Verify a receipt yourself">
        <p className="max-w-2xl text-muted">
          Each decision is hashed. The receipt page has a Verify hash button,
          and the reference verifier needs only the Python standard library.
        </p>
        <Code>{`python3 tools/recompute_receipt.py proofs/receipts/slv_<id>.json
# the printed hash must equal evidence_sha256 on the receipt`}</Code>
      </Block>

      <Block id="selfhost" title="Self-host">
        <Code>{`cp .env.example .env     # Cloudflare, SERV and Reppo keys
cargo run                 # one binary, no database`}</Code>
        <p className="text-sm text-muted">
          Without provider keys every check holds on{" "}
          <span className="font-mono">SOLV-SYSTEM-003</span>, by design.
        </p>
      </Block>
    </Page>
  );
}

// ---------------------------------------------------------------------------

const COST_ROWS: [string, string, string][] = [
  ["Gate logic", "Every check", "24 µs measured with mock detectors."],
  ["Tier 1 small model", "Every check", "One call per check. Median 394 ms (12 requests)."],
  ["Tier 2 escalation", "Unrated or uncertain checks", "Ran in all 34 measured checks. About 3.7 s per escalated check."],
  ["Reppo evaluation", "Off the request path", "440 credits, about $0.22 (1 USDC buys 2,000). A new Reppo account starts with 22,000 credits once."],
];

export function PricingPage(): JSX.Element {
  return (
    <Page
      title="Cost model"
      lede="Solv has no billing, so there is no price list. This is what a check costs to run."
    >
      <Block id="costs" title="What a check costs to run">
        <DataTable
          head={["Component", "When it runs", "Cost basis"]}
          rows={COST_ROWS.map(([component, when, basis]) => [
            <span key="c" className="font-medium">{component}</span>,
            <span key="w" className="text-muted">{when}</span>,
            <span key="b" className="text-muted">{basis}</span>,
          ])}
        />
        <Unproven>
          The blended cost per check, the escalation split, and the price of
          Reppo credits beyond the free tier are unmeasured or unpublished.
        </Unproven>
      </Block>

      <Block id="shape" title="What a price could look like">
        <p className="max-w-2xl text-muted">
          Per check for operators, or a periodic audit for providers. Neither
          can be quoted before these numbers exist.
        </p>
      </Block>
    </Page>
  );
}

// ---------------------------------------------------------------------------

const ATTACK_CATEGORIES: [string, string][] = [
  ["Direct instruction", "Text addressed to the agent: ignore your rules, send to this address."],
  ["Cold start posing as established", "Claims of long history or a spotless record."],
  ["Urgency and coercion", "Deadlines, threats of loss, penalties for waiting."],
  ["Exfiltration", "Requests for keys, tokens, prompts or state."],
  ["Encoding layers", "Base64, zero-width characters and homoglyphs hiding the payload."],
  ["Claim and evidence mismatch", "A service text that promises what no process could deliver."],
];

export function AttackLabPage(): JSX.Element {
  return (
    <Page
      title="Attack Lab"
      lede="Twenty hostile cases in six categories. None produces an approval, and the suite fails if that changes."
    >
      <Block id="categories" title="The six categories">
        <div className="grid gap-4 md:grid-cols-2">
          {ATTACK_CATEGORIES.map(([name, body]) => (
            <div key={name} className="rounded-xl bg-surface p-5">
              <h3 className="font-semibold">{name}</h3>
              <p className="mt-1 text-sm text-muted">{body}</p>
            </div>
          ))}
        </div>
      </Block>

      <Block id="run" title="Run it">
        <Code>{`cargo test --test attack`}</Code>
        <p className="text-sm text-muted">
          Offline. No network, no credentials, no provider credits.
        </p>
      </Block>

      <Block id="limits" title="What this does not prove">
        <Unproven>
          The corpus runs against provider stand-ins, so live behaviour on
          encoding layers is untested. There has been no third-party audit.
        </Unproven>
        <p className="text-sm text-muted">
          <Link to="/how-it-works" className="underline underline-offset-2">
            How it works
          </Link>{" "}
          lists what the gate guarantees by structure, which is weaker than
          "cannot be attacked".
        </p>
      </Block>
    </Page>
  );
}
