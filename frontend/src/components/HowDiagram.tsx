// The landing diagram: how a payment check runs, and where Cloudflare,
// OpenServ and Reppo sit in it. Two lanes. Lane one is the request path, the
// only thing a paying agent waits for. Lane two runs ahead of time and ends in
// the score profile that lane one reads.
//
// Every time shown is either measured (with its sample size in the footnote on
// the landing page) or stated as a limit. Nothing here is a target.

import type { JSX, ReactNode } from "react";

import {
  ArrowDown,
  ArrowRight,
  IconAgent,
  IconCloud,
  IconFilter,
  IconGate,
  IconNodes,
  IconProfile,
  IconQueue,
  IconReceipt,
  IconScale,
} from "./Icons";

type Who = "provider" | "solv";

interface Step {
  title: string;
  who: Who;
  /** Provider name shown above the title, when an outside service does the work. */
  provider?: string;
  body: string;
  icon: ReactNode;
  time: string;
  /** `measured` numbers come from live runs; `note` is a limit or a rule. */
  kind: "measured" | "note";
}

const REQUEST_LANE: Step[] = [
  {
    title: "Your agent asks first",
    who: "solv",
    body: "Before paying, it sends the name, URL, amount and the service text.",
    icon: <IconAgent />,
    time: "1 HTTP call",
    kind: "note",
  },
  {
    title: "Tier 1: screen the text",
    who: "provider",
    provider: "Cloudflare Clef-flash",
    body: "A small model on Cloudflare Workers AI reads the service text as data and scores injection and risk.",
    icon: <IconCloud />,
    time: "0.4 s median",
    kind: "measured",
  },
  {
    title: "Tier 2: reason about it",
    who: "provider",
    provider: "OpenServ SERV",
    body: "SERV Reasoning is skipped when Tier 1 is certain of an injection. It ran in every clean live check.",
    icon: <IconNodes />,
    time: "3 to 4 s",
    kind: "measured",
  },
  {
    title: "Policy gate",
    who: "solv",
    body: "Plain Rust rules, no model. The score band, spend cap and hard stops decide.",
    icon: <IconGate />,
    time: "24 µs",
    kind: "measured",
  },
  {
    title: "Decision and receipt",
    who: "solv",
    body: "APPROVE, HOLD or BLOCK with reason codes, and a public receipt.",
    icon: <IconReceipt />,
    time: "public address",
    kind: "note",
  },
];

const AUDIT_LANE: Step[] = [
  {
    title: "An operator queues an audit",
    who: "solv",
    body: "A counterparty name, its URL and its public claim go into a queue.",
    icon: <IconQueue />,
    time: "on demand",
    kind: "note",
  },
  {
    title: "Independent evaluation",
    who: "provider",
    provider: "Reppo Eval Router",
    body: "A panel of evaluator models scores the claim and reports how much they agree. Cost: 440 credits, about $0.22.",
    icon: <IconScale />,
    time: "2 to 5 min",
    kind: "measured",
  },
  {
    title: "Confidence check",
    who: "solv",
    body: "A low-confidence result is charged but never becomes a score.",
    icon: <IconFilter />,
    time: "instant",
    kind: "note",
  },
  {
    title: "Score profile",
    who: "solv",
    body: "300 to 850, aged and dropped after 90 days. The gate reads it from memory.",
    icon: <IconProfile />,
    time: "read in microseconds",
    kind: "note",
  },
];

function TimeBadge({ step }: { step: Step }): JSX.Element {
  const measured = step.kind === "measured";
  return (
    <span
      className={`mt-auto inline-flex w-fit items-center gap-1.5 whitespace-nowrap rounded-full border px-2.5 py-0.5 font-mono text-xs ${
        measured
          ? "border-approve-ink/40 bg-approve-bg text-approve-ink"
          : "border-border bg-wash text-muted"
      }`}
    >
      {measured && <span className="font-semibold">MEASURED</span>}
      {step.time}
    </span>
  );
}

function StepCard({ step, index }: { step: Step; index: number }): JSX.Element {
  const provider = step.who === "provider";
  return (
    <div
      className={`flex h-full flex-col gap-3 rounded-xl border bg-surface p-4 ${
        provider ? "border-accent border-t-4" : "border-border"
      }`}
    >
      <div className="flex items-center justify-between">
        <span
          className={`flex h-10 w-10 items-center justify-center rounded-lg ${
            provider ? "bg-accent-soft text-accent-ink" : "bg-wash text-ink"
          }`}
        >
          {step.icon}
        </span>
        <span className="font-mono text-xs text-muted">
          {String(index + 1).padStart(2, "0")}
        </span>
      </div>
      <div>
        {step.provider && (
          <p className="font-mono text-[0.7rem] font-medium uppercase tracking-wide text-accent-ink">
            {step.provider}
          </p>
        )}
        <h3 className="mt-0.5 text-base font-semibold leading-snug">{step.title}</h3>
      </div>
      <p className="text-sm text-muted">{step.body}</p>
      <TimeBadge step={step} />
    </div>
  );
}

function Lane({
  label,
  caption,
  steps,
  columns,
}: {
  label: string;
  caption: string;
  steps: Step[];
  columns: "lg:grid-cols-5" | "lg:grid-cols-4";
}): JSX.Element {
  return (
    <div className="rounded-2xl border border-border bg-wash/60 p-4 sm:p-6">
      <div className="mb-4 flex flex-wrap items-baseline justify-between gap-x-4 gap-y-1">
        <h3 className="text-lg font-bold">{label}</h3>
        <p className="text-sm text-muted">{caption}</p>
      </div>
      <ol className={`grid gap-3 lg:gap-7 ${columns}`}>
        {steps.map((step, index) => (
          <li key={step.title} className="relative">
            <StepCard step={step} index={index} />
            {index < steps.length - 1 && (
              <>
                <span
                  aria-hidden
                  className="absolute -right-[1.35rem] top-1/2 hidden -translate-y-1/2 text-accent lg:block"
                >
                  <ArrowRight />
                </span>
                <span
                  aria-hidden
                  className="flex justify-center pt-1 text-accent lg:hidden"
                >
                  <ArrowDown />
                </span>
              </>
            )}
          </li>
        ))}
      </ol>
    </div>
  );
}

export function HowDiagram(): JSX.Element {
  return (
    <div>
      <div className="mb-4 flex flex-wrap items-center gap-x-5 gap-y-2 text-sm text-muted">
        <span className="flex items-center gap-2">
          <span className="inline-block h-3 w-6 rounded-sm border border-accent border-t-4 bg-surface" />
          Outside service
        </span>
        <span className="flex items-center gap-2">
          <span className="inline-block h-3 w-6 rounded-sm border border-border bg-surface" />
          Solv code
        </span>
        <span className="flex items-center gap-2">
          <span className="rounded-full border border-approve-ink/40 bg-approve-bg px-2 font-mono text-xs font-semibold text-approve-ink">
            MEASURED
          </span>
          from live runs, small samples
        </span>
      </div>

      <Lane
        label="Every payment check"
        caption="The agent waits for this. A certain injection is blocked in about a second or less."
        steps={REQUEST_LANE}
        columns="lg:grid-cols-5"
      />

      <div
        aria-hidden
        className="flex items-center justify-center gap-3 py-3 text-sm text-muted"
      >
        <span className="h-6 border-l-2 border-dashed border-accent" />
        <span className="max-w-xl text-center">
          The gate reads the score profile on every check. Reppo is never called here.
        </span>
        <span className="h-6 border-l-2 border-dashed border-accent" />
      </div>

      <Lane
        label="Rating a counterparty"
        caption="Runs ahead of time, off the request path."
        steps={AUDIT_LANE}
        columns="lg:grid-cols-4"
      />
    </div>
  );
}
