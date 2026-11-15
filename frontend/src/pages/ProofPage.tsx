// `/proof` — one live example per audience, each stating what to expect before
// it runs and then comparing the real answer with that expectation. Every run
// is a real request to the real providers through the same client the desk
// uses. When something the example needs is missing (the demo profile, for
// instance) the page says so instead of failing silently.

import { useCallback, useEffect, useState } from "react";
import type { JSX, ReactNode } from "react";
import { Link, useLocation } from "react-router-dom";

import { ApiError, fetchDirectory, preSpendCheck } from "../api";
import { DecisionBadge } from "../components/Badges";
import type { Decision } from "../components/Badges";
import { CounterpartyDetail } from "../components/CounterpartyCheck";
import { ms } from "../format";
import { PRESETS } from "../presets";
import type { Preset } from "../presets";
import type { CounterpartyEntry, PreSpendCheckResponse } from "../types";

const DEMO_COUNTERPARTY = "github-rest-api";

const TABS = [
  { id: "operators", label: "Agent operators and wallets" },
  { id: "marketplaces", label: "Marketplaces and bounty platforms" },
  { id: "providers", label: "API and MCP providers" },
] as const;

type TabId = (typeof TABS)[number]["id"];

function tabFromHash(hash: string): TabId {
  const wanted = hash.replace("#", "");
  return TABS.find((tab) => tab.id === wanted)?.id ?? "operators";
}

type Run =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "done"; result: PreSpendCheckResponse; wallMs: number }
  | { kind: "error"; message: string };

function useRun(): [Run, (preset: Preset) => void] {
  const [run, setRun] = useState<Run>({ kind: "idle" });
  const start = useCallback((preset: Preset) => {
    setRun({ kind: "running" });
    const began = performance.now();
    preSpendCheck({
      counterparty_name: preset.counterparty_name,
      target_url: preset.target_url,
      amount_usd: preset.amount_usd,
      service_description: preset.service_description,
    })
      .then((result) =>
        setRun({ kind: "done", result, wallMs: performance.now() - began }),
      )
      .catch((error: unknown) => {
        const message =
          error instanceof ApiError
            ? error.status === 429
              ? "Too many checks from this address. Wait a minute and try again."
              : error.message
            : "The service did not answer.";
        setRun({ kind: "error", message });
      });
  }, []);
  return [run, start];
}

function curlFor(preset: Preset): string {
  const body = JSON.stringify(
    {
      counterparty_name: preset.counterparty_name,
      target_url: preset.target_url,
      amount_usd: preset.amount_usd,
      service_description: preset.service_description,
    },
    null,
    2,
  );
  return (
    `curl -X POST ${window.location.origin}/api/v1/pre-spend-check \\\n` +
    `  -H 'Content-Type: application/json' \\\n` +
    `  -d '${body.replace(/'/g, "'\\''")}'`
  );
}

function CodeBlock({ children }: { children: string }): JSX.Element {
  return (
    <pre className="overflow-x-auto rounded-lg bg-wash p-3 font-mono text-xs leading-relaxed">
      {children}
    </pre>
  );
}

const RUN_BUTTON =
  "rounded-lg bg-ink px-4 py-2 text-sm font-semibold text-bg transition-opacity hover:opacity-90 disabled:opacity-50";

function ResultView({
  run,
  expected,
  hint,
}: {
  run: Run;
  expected: Decision;
  hint?: (result: PreSpendCheckResponse) => string | null;
}): JSX.Element | null {
  if (run.kind === "idle") return null;
  if (run.kind === "running") {
    return (
      <p role="status" className="mt-4 text-sm text-muted">
        Calling the providers…
      </p>
    );
  }
  if (run.kind === "error") {
    return (
      <p role="alert" className="mt-4 rounded-lg bg-block-solid p-3 text-sm text-white">
        {run.message}
      </p>
    );
  }
  const { result, wallMs } = run;
  const matches = result.decision === expected;
  const note = hint?.(result) ?? null;
  return (
    <div className="mt-4 space-y-3" aria-live="polite">
      <p
        className={`rounded-lg p-3 text-sm ${
          matches ? "bg-approve-solid text-white" : "bg-hold-solid text-ink"
        }`}
      >
        Expected {expected}. Got {result.decision}.{" "}
        {matches ? "It matches." : "It differs."}
      </p>
      {note && <p className="text-sm">{note}</p>}
      <div className="flex flex-wrap items-center gap-x-4 gap-y-2">
        <DecisionBadge decision={result.decision} />
        <span className="font-mono text-sm">{ms(wallMs)} round trip</span>
        <span className="font-mono text-xs text-muted">
          {result.tier_used.join(" + ") || "no model call"} · tier 1 {ms(result.latency_ms.tier1)}
          {result.latency_ms.tier2 !== undefined && ` · tier 2 ${ms(result.latency_ms.tier2)}`}
        </span>
      </div>
      <ul className="space-y-2">
        {result.reasons.map((reason, index) => (
          <li key={`${reason.code}-${index}`} className="rounded-lg bg-surface p-3">
            <p className="font-mono text-xs font-medium">{reason.code}</p>
            <p className="mt-0.5 text-sm text-muted">{reason.message}</p>
          </li>
        ))}
      </ul>
      <a
        href={result.receipt_url}
        className="inline-block text-sm font-semibold text-accent-ink underline underline-offset-4"
      >
        Open the receipt
      </a>
    </div>
  );
}

function ExampleRun({
  preset,
  expected,
  expectText,
  hint,
}: {
  preset: Preset;
  expected: Decision;
  expectText: string;
  hint?: (result: PreSpendCheckResponse) => string | null;
}): JSX.Element {
  const [run, start] = useRun();
  return (
    <article className="rounded-xl bg-surface p-5">
      <h3 className="text-lg font-semibold">{preset.name}</h3>
      <p className="mt-1 text-sm text-muted">
        <span className="font-medium text-ink">Expect:</span> {expectText}
      </p>
      <button
        type="button"
        className={`mt-4 ${RUN_BUTTON}`}
        disabled={run.kind === "running"}
        onClick={() => start(preset)}
      >
        {run.kind === "running" ? "Running…" : "Run this check"}
      </button>
      <ResultView run={run} expected={expected} hint={hint} />
      <details className="mt-4">
        <summary className="cursor-pointer text-sm font-medium">The same call with curl</summary>
        <div className="mt-2">
          <CodeBlock>{curlFor(preset)}</CodeBlock>
        </div>
      </details>
    </article>
  );
}

function presetById(id: string): Preset {
  const found = PRESETS.find((preset) => preset.id === id);
  if (!found) throw new Error(`preset ${id} is missing`);
  return found;
}

function Gets({ children }: { children: ReactNode }): JSX.Element {
  return (
    <p className="max-w-2xl text-lg">
      <span className="font-semibold">What you get. </span>
      {children}
    </p>
  );
}

function OperatorsPanel(): JSX.Element {
  return (
    <div className="space-y-6">
      <Gets>One HTTP call before each payment: a decision, the reasons, and a receipt.</Gets>
      <div className="grid gap-4 lg:grid-cols-2">
        <ExampleRun
          preset={presetById("injection")}
          expected="BLOCK"
          expectText="BLOCK in about a second or less. Tier 1 is certain, so Tier 2 is skipped."
        />
        <ExampleRun
          preset={presetById("prime")}
          expected="APPROVE"
          expectText="APPROVE in 3 to 5 seconds, with the reasons. It needs the demo profile."
          hint={(result) =>
            result.reasons.some((reason) => reason.code === "SOLV-COLDSTART-001")
              ? "The demo profile is not loaded on this server, so this became a cold-start HOLD. Start Solv with SOLV_SEED_FILE=seed/demo_profiles.json."
              : null
          }
        />
      </div>
    </div>
  );
}

function MarketplacesPanel(): JSX.Element {
  const preset = presetById("injection");
  const [run, start] = useRun();
  return (
    <div className="space-y-6">
      <Gets>Every decision becomes a public receipt you can check without trusting Solv.</Gets>
      <ol className="grid gap-4 md:grid-cols-3">
        {[
          ["1", "Run a check", "Release funds only after the gate answers. Each answer is stored."],
          ["2", "Open the receipt", "The receipt shows the decision, the reasons, the policy version and a hash."],
          ["3", "Verify the hash", "Press Verify hash. Your browser recomputes the SHA-256 of the evidence and compares it."],
        ].map(([n, title, body]) => (
          <li key={n} className="rounded-xl bg-surface p-5">
            <span className="font-mono text-xs text-accent-ink">STEP {n}</span>
            <h3 className="mt-1 text-lg font-semibold">{title}</h3>
            <p className="mt-1 text-sm text-muted">{body}</p>
          </li>
        ))}
      </ol>
      <article className="rounded-xl bg-surface p-5">
        <p className="text-sm text-muted">
          <span className="font-medium text-ink">Expect:</span> a BLOCK within about a second, then a
          receipt page with a Verify hash button that reports a match.
        </p>
        <button
          type="button"
          className={`mt-4 ${RUN_BUTTON}`}
          disabled={run.kind === "running"}
          onClick={() => start(preset)}
        >
          {run.kind === "running" ? "Running…" : "Create a receipt"}
        </button>
        {run.kind === "error" && (
          <p role="alert" className="mt-4 text-sm text-block-ink">
            {run.message}
          </p>
        )}
        {run.kind === "done" && (
          <dl className="mt-4 grid grid-cols-[auto_1fr] gap-x-4 gap-y-2 text-sm" aria-live="polite">
            <dt className="text-muted">Decision</dt>
            <dd>
              <DecisionBadge decision={run.result.decision} />
            </dd>
            <dt className="text-muted">Policy version</dt>
            <dd className="font-mono">ruleset v{run.result.ruleset_version}</dd>
            <dt className="text-muted">Evidence hash</dt>
            <dd className="break-all font-mono text-xs">{run.result.evidence_sha256}</dd>
            <dt className="text-muted">Receipt</dt>
            <dd>
              <a
                href={run.result.receipt_url}
                className="font-semibold text-accent-ink underline underline-offset-4"
              >
                Open it and press Verify hash
              </a>
            </dd>
          </dl>
        )}
      </article>
    </div>
  );
}

type Directory =
  | { kind: "loading" }
  | { kind: "error" }
  | { kind: "ready"; entries: CounterpartyEntry[] };

function ProvidersPanel(): JSX.Element {
  const [directory, setDirectory] = useState<Directory>({ kind: "loading" });
  const [run, start] = useRun();
  const load = useCallback(() => {
    fetchDirectory()
      .then((response) => setDirectory({ kind: "ready", entries: response.counterparties }))
      .catch(() => setDirectory({ kind: "error" }));
  }, []);
  useEffect(load, [load]);
  // A finished check writes a receipt; reload so the profile can show the
  // evaluation id and pillars that the receipt carries.
  useEffect(() => {
    if (run.kind === "done") load();
  }, [run.kind, load]);

  const present =
    directory.kind === "ready" &&
    directory.entries.some((entry) => entry.counterparty_id === DEMO_COUNTERPARTY);

  return (
    <div className="space-y-6">
      <Gets>A score from an independent evaluation and a public page you can show to buyers.</Gets>

      <article className="rounded-xl bg-surface p-5">
        <p className="text-sm text-muted">
          <span className="font-medium text-ink">Expect:</span> the profile of{" "}
          <span className="font-mono">{DEMO_COUNTERPARTY}</span> with its band, score, and the age
          and expiry of the score. Run one check to also load its pillars and the Reppo evaluation
          id.
        </p>
        <div className="mt-4">
          {directory.kind === "loading" && <p className="text-sm text-muted">Loading the registry…</p>}
          {directory.kind === "error" && (
            <p role="alert" className="text-sm text-block-ink">
              The registry did not answer.
            </p>
          )}
          {directory.kind === "ready" && !present && (
            <p className="rounded-lg bg-hold-solid p-3 text-sm text-ink">
              The demo profile is not loaded on this server. Start Solv with
              SOLV_SEED_FILE=seed/demo_profiles.json to see a rated profile here.
            </p>
          )}
          {directory.kind === "ready" && present && (
            <CounterpartyDetail id={DEMO_COUNTERPARTY} entries={directory.entries} />
          )}
        </div>
        {present && (
          <div className="mt-4 flex flex-wrap items-center gap-x-5 gap-y-2">
            <button
              type="button"
              className={RUN_BUTTON}
              disabled={run.kind === "running"}
              onClick={() => start(presetById("prime"))}
            >
              {run.kind === "running" ? "Running…" : "Run a check to load the evidence"}
            </button>
            <Link
              to="/explorer"
              className="text-sm font-semibold text-accent-ink underline underline-offset-4"
            >
              Search the registry
            </Link>
          </div>
        )}
        {run.kind === "error" && (
          <p role="alert" className="mt-3 text-sm text-block-ink">
            {run.message}
          </p>
        )}
      </article>

      <div>
        <h3 className="text-lg font-semibold">How to get rated</h3>
        <ol className="mt-3 grid gap-4 md:grid-cols-3">
          {[
            ["1", "Send your details", "The operator needs your name, URL and a public claim about your service."],
            ["2", "An audit runs", "Reppo evaluates the claim. It takes 2 to 5 minutes and costs the operator 440 credits."],
            ["3", "Your profile appears", "The score shows in the registry and expires after 90 days unless it is renewed."],
          ].map(([n, title, body]) => (
            <li key={n} className="rounded-xl bg-surface p-5">
              <span className="font-mono text-xs text-accent-ink">STEP {n}</span>
              <h4 className="mt-1 font-semibold">{title}</h4>
              <p className="mt-1 text-sm text-muted">{body}</p>
            </li>
          ))}
        </ol>
        <p className="mt-4 max-w-2xl text-sm text-muted">
          Audits are run by the operator for now. There is no self-service form yet. Scores attach
          to a claimed name and URL, not to a verified identity.
        </p>
      </div>
    </div>
  );
}

export function ProofPage(): JSX.Element {
  const location = useLocation();
  const active = tabFromHash(location.hash);

  return (
    <div className="wrap py-14">
      <h1 className="text-4xl font-bold sm:text-5xl">See it work</h1>
      <p className="mt-3 max-w-2xl text-lg text-muted">
        One live example for each audience. Each one says what to expect, then runs for real.
      </p>

      <div role="tablist" aria-label="audiences" className="mt-8 flex flex-wrap gap-2">
        {TABS.map((tab) => (
          <Link
            key={tab.id}
            id={`tab-${tab.id}`}
            to={{ hash: `#${tab.id}` }}
            replace
            role="tab"
            aria-selected={tab.id === active}
            aria-controls={`panel-${tab.id}`}
            className={`rounded-lg px-4 py-2 text-sm font-semibold ${
tab.id === active
              ? "bg-ink text-bg"
              : "bg-surface text-muted hover:text-ink"
          }`}
          >
            {tab.label}
          </Link>
        ))}
      </div>

      <div
        role="tabpanel"
        id={`panel-${active}`}
        aria-labelledby={`tab-${active}`}
        className="mt-8"
      >
        {active === "operators" && <OperatorsPanel />}
        {active === "marketplaces" && <MarketplacesPanel />}
        {active === "providers" && <ProvidersPanel />}
      </div>
    </div>
  );
}
