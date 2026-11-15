// `/` — The Underwriting Desk (docs/DESIGN.md §4.1). A visitor should grasp
// what Solv does within five seconds: hero metrics measured from real
// receipts, a pre-spend simulator on the left, the live decision pipeline on
// the right. Every input produces either a decision with reasons or an
// honest error — never a confident guess.

import { useCallback, useEffect, useState } from "react";
import type { FormEvent, JSX } from "react";

import { fetchDirectory, preSpendCheck, ApiError } from "../api";
import { usd } from "../format";
import { PRESETS } from "../presets";
import type { Preset } from "../presets";
import { MetricTile } from "../components/MetricTile";
import { Pipeline } from "../components/Pipeline";
import { ReasonList } from "../components/ReasonList";
import { ScorePanel } from "../components/ScorePanel";
import type { DirectoryMetrics, PreSpendCheckResponse } from "../types";

type FormState = {
  counterparty_name: string;
  target_url: string;
  amount_usd: string;
  service_description: string;
};

const EMPTY_FORM: FormState = {
  counterparty_name: "",
  target_url: "",
  amount_usd: "",
  service_description: "",
};

type Status =
  | { kind: "idle" }
  | { kind: "running" }
  | { kind: "done"; result: PreSpendCheckResponse }
  | { kind: "error"; message: string };

export function DeskPage(): JSX.Element {
  const [form, setForm] = useState<FormState>(EMPTY_FORM);
  const [activePreset, setActivePreset] = useState<string | null>(null);
  const [status, setStatus] = useState<Status>({ kind: "idle" });
  const [metrics, setMetrics] = useState<DirectoryMetrics | null>(null);
  const [metricsFailed, setMetricsFailed] = useState(false);

  const loadMetrics = useCallback(() => {
    fetchDirectory()
      .then((directory) => {
        setMetrics(directory.metrics);
        setMetricsFailed(false);
      })
      // The strip is a read-only flourish: a failed fetch shows an honest
      // "metrics unavailable" and the simulator keeps working.
      .catch(() => {
        setMetrics(null);
        setMetricsFailed(true);
      });
  }, []);

  useEffect(loadMetrics, [loadMetrics]);

  function applyPreset(preset: Preset) {
    setActivePreset(preset.id);
    setForm({
      counterparty_name: preset.counterparty_name,
      target_url: preset.target_url,
      amount_usd: String(preset.amount_usd),
      service_description: preset.service_description,
    });
  }

  const updateForm = (patch: Partial<FormState>) => {
    setActivePreset(null); // manual edits release the preset highlight
    setForm((current) => ({ ...current, ...patch }));
  };

  async function onSubmit(event: FormEvent) {
    event.preventDefault();
    const amountText = form.amount_usd.trim();
    // Strict grammar before parsing: "12abc" is a typo, not a donation of 12.
    const amountOk = /^\d+(\.\d{1,2})?$/.test(amountText);
    const amount = amountOk ? Number.parseFloat(amountText) : Number.NaN;
    if (
      form.counterparty_name.trim() === "" ||
      form.target_url.trim() === "" ||
      form.service_description.trim() === "" ||
      !amountOk ||
      amount <= 0
    ) {
      setStatus({
        kind: "error",
        message:
          "Fill every field; amount must be a positive number with at most two decimals.",
      });
      return;
    }
    setStatus({ kind: "running" });
    try {
      const result = await preSpendCheck({
        counterparty_name: form.counterparty_name,
        target_url: form.target_url,
        amount_usd: amount,
        service_description: form.service_description,
      });
      setStatus({ kind: "done", result });
      loadMetrics();
    } catch (error) {
      setStatus({
        kind: "error",
        message:
          error instanceof ApiError
            ? `The gate refused the request: ${error.message}`
            : "The gate could not be reached. It decides fail-closed, so treat this as HOLD.",
      });
    }
  }

  const result = status.kind === "done" ? status.result : null;
  const blocked =
    metrics === null || metrics.total_blocked_usd <= 0
      ? "—"
      : usd(metrics.total_blocked_usd);

  return (
    <div className="wrap py-8">
      {/* Hero metric strip — real receipts or an honest dash. */}
      <div className="grid grid-cols-1 gap-3 sm:grid-cols-3">
        <MetricTile
          label="Fraud blocked, cumulative"
          value={blocked}
          verification="MEASURED"
          detail={metricsFailed ? "metrics unavailable" : "sum over BLOCK receipts on this instance"}
        />
        <MetricTile
          label="Tier-1 latency, median"
          value={
            metrics?.latency_ms.tier1_only_p50 !== null &&
            metrics?.latency_ms.tier1_only_p50 !== undefined
              ? `${Math.round(metrics.latency_ms.tier1_only_p50)}`
              : "—"
          }
          unit="ms"
          verification="MEASURED"
          detail={
            metricsFailed
              ? "metrics unavailable"
              : "p50 over Tier-1-only receipts; a dash means none finished at Tier 1 alone"
          }
        />
        <MetricTile
          label="Cost per check"
          value="—"
          verification="UNPROVEN"
          detail="receipts carry no token counts yet"
        />
      </div>

      <div className="mt-6 grid grid-cols-1 gap-6 lg:grid-cols-[26rem_1fr]">
        {/* Left — the simulator. */}
        <section aria-labelledby="simulator-title" className="bg-surface">
          <div className="p-5">
            <h2 id="simulator-title" className="font-head text-xl font-bold">
              Pre-spend simulator
            </h2>
            <p className="mt-1 text-sm text-muted">
              The call an agent makes just before it pays. Names and URLs are
              stored verbatim on a public receipt. Send only what you would
              publish.
            </p>
          </div>

          <div className="flex flex-col gap-2 p-5 pb-0 sm:flex-row">
            {PRESETS.map((preset) => (
              <button
                key={preset.id}
                type="button"
                onClick={() => applyPreset(preset)}
                aria-pressed={activePreset === preset.id}
                title={preset.demonstrates}
                className={`grow rounded-lg px-2 py-2 text-left text-[13px] leading-tight transition-colors focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
activePreset === preset.id ? "bg-accent-fill text-white" : "bg-bg"
                }`}
              >
                {preset.name}
              </button>
            ))}
          </div>
          <p className="px-5 pt-2 font-mono text-xs text-muted">
            {activePreset
              ? PRESETS.find((preset) => preset.id === activePreset)?.demonstrates
              : "pick a preset or write your own below"}
          </p>

          <form onSubmit={(event) => void onSubmit(event)} className="p-5">
            <label className="block text-sm text-muted" htmlFor="f-name">
              Counterparty name
            </label>
            <input
              id="f-name"
              required
              value={form.counterparty_name}
              onChange={(e) => updateForm({ counterparty_name: e.target.value })}
              className="mt-1 w-full bg-bg px-2.5 py-1.5 font-mono text-sm focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
            />

            <label className="mt-4 block text-sm text-muted" htmlFor="f-url">
              Target URL
            </label>
            <input
              id="f-url"
              type="url"
              required
              placeholder="https://"
              value={form.target_url}
              onChange={(e) => updateForm({ target_url: e.target.value })}
              className="mt-1 w-full bg-bg px-2.5 py-1.5 font-mono text-sm focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
            />

            <label className="mt-4 block text-sm text-muted" htmlFor="f-amount">
              Amount (USD)
            </label>
            <input
              id="f-amount"
              required
              inputMode="decimal"
              min="0.01"
              step="0.01"
              value={form.amount_usd}
              onChange={(e) => updateForm({ amount_usd: e.target.value })}
              className="mt-1 w-36 bg-bg px-2.5 py-1.5 font-mono text-sm tabular-nums focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
            />

            <label className="mt-4 block text-sm text-muted" htmlFor="f-desc">
              Service description <span className="font-mono text-xs">(untrusted text)</span>
            </label>
            <textarea
              id="f-desc"
              required
              rows={7}
              value={form.service_description}
              onChange={(e) => updateForm({ service_description: e.target.value })}
              className="mt-1 w-full resize-y bg-bg px-2.5 py-1.5 font-mono text-[13px] leading-relaxed focus-visible:outline-2 focus-visible:outline-offset-1 focus-visible:outline-accent"
            />

            <button
              type="submit"
              disabled={status.kind === "running"}
              className="mt-4 w-full rounded-lg bg-accent-fill px-4 py-2.5 font-head text-sm font-medium uppercase tracking-wider text-white transition-opacity hover:opacity-90 disabled:cursor-wait disabled:opacity-60 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
            >
              {status.kind === "running" ? "Gate deciding…" : "Run pre-spend check"}
            </button>

            {status.kind === "error" && (
              <p role="alert" className="mt-3 rounded-lg bg-hold-solid px-3 py-2 text-sm text-ink">
                {status.message}
              </p>
            )}
          </form>
        </section>

        {/* Right — the live pipeline and the decision it produced. */}
        <div
          className="flex min-w-0 flex-col gap-6"
          role="status"
          aria-live="polite"
        >
          <Pipeline result={result} running={status.kind === "running"} />

          {result && (
            <>
              <section
                aria-labelledby="stamp-title"
                className={`rounded-xl p-5 ${STAMP_CLASS[result.decision]}`}
              >
                <h2 id="stamp-title" className="font-head text-2xl font-bold tracking-tight">
                  {STAMP_TEXT[result.decision](usd(result.max_safe_spend))}
                </h2>
                <p className="mt-1 text-sm">
                  {DECISION_NOTE[result.decision]} Max safe spend:{" "}
                  <span className="font-mono">{usd(result.max_safe_spend)}</span>.
                </p>
              </section>

              <ScorePanel
                tier={result.tier}
                fico={result.fico_score}
                raw={result.fico_raw}
                decay={result.decay}
                ageDays={result.score_age_days}
                pillars={result.pillars}
                source={result.score_source}
              />

              <section aria-labelledby="reasons-title" className="bg-surface p-5">
                <h2 id="reasons-title" className="font-head text-xl font-bold">
                  Reasons
                </h2>
                <p className="mt-0.5 mb-3 text-sm text-muted">
                  ruleset v{result.ruleset_version} · evidence{" "}
                  <span className="font-mono text-xs">{result.evidence_sha256}</span>
                </p>
                <ReasonList reasons={result.reasons} />
              </section>
            </>
          )}

          {!result && status.kind !== "running" && (
            <section className="bg-surface p-5 text-sm text-muted">
              No decision in this session yet. Solv answers APPROVE, HOLD or
              BLOCK. An APPROVE is explained with the same care as a BLOCK.
            </section>
          )}
        </div>
      </div>
    </div>
  );
}

const STAMP_CLASS: Record<PreSpendCheckResponse["decision"], string> = {
  APPROVE: "bg-approve-solid text-white",
  HOLD: "bg-hold-solid text-ink",
  BLOCK: "bg-block-solid text-white",
};

const STAMP_TEXT: Record<PreSpendCheckResponse["decision"], (amount: string) => string> = {
  APPROVE: (amount) => `APPROVED — ${amount}`,
  HOLD: (amount) => `HELD — ${amount} NOT AUTHORISED`,
  BLOCK: (amount) => `BLOCKED — ${amount} NOT AUTHORISED`,
};

const DECISION_NOTE: Record<PreSpendCheckResponse["decision"], string> = {
  APPROVE: "The gate recorded approval; the payment path may proceed inside the cap.",
  HOLD: "Not enough settled evidence. Human review, or wait for an evaluation to settle.",
  BLOCK: "A forcing reason applies. The gate will not approve this spend.",
};
