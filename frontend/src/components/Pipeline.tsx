// The live decision pipeline (DESIGN §4.1): one row per stage, real elapsed
// times from the receipt's latency_ms, skipped stages visible with their
// reason. A skipped stage is never hidden — hiding it would misrepresent the
// architecture, and the architecture is the pitch.
//
// Honesty rule (review finding): `tier_used` names the tiers that *ran*, not
// the tiers that produced a verdict. Provider failure is carried by the
// gate's SYSTEM reason codes — the only SYSTEM codes the policy gate emits
// are SOLV-SYSTEM-002 (latency ceiling) and SOLV-SYSTEM-003 (no decision
// layer reachable), so those two drive the ✓ / ✗ states here.

import type { JSX } from "react";

import type { PreSpendCheckResponse } from "../types";
import { ms } from "../format";
import { DecisionBadge } from "./Badges";

type NodeState = "ok" | "unavailable" | "skipped" | "idle";

interface Node {
  name: string;
  state: NodeState;
  elapsed: number | null;
  detail?: string;
}

function idleNodes(): Node[] {
  return [
    { name: "State lookup", state: "idle", elapsed: null },
    { name: "Clef-flash · Tier 1", state: "idle", elapsed: null },
    { name: "SERV Reasoning · Tier 2", state: "idle", elapsed: null },
    { name: "Policy gate", state: "idle", elapsed: null },
  ];
}

function nodesFor(r: PreSpendCheckResponse): Node[] {
  const hasReason = (code: string) => r.reasons.some((reason) => reason.code === code);
  const tier1Dead = hasReason("SOLV-SYSTEM-003");
  const ceilingHit = hasReason("SOLV-SYSTEM-002");
  const tier2Ran = r.tier_used.includes("serv-reasoning");

  const tier2Elapsed = r.latency_ms.tier2 ?? 0;
  // The gate's own time is what the whole path spent minus what the tiers
  // spent; deterministic arithmetic, not a new claim.
  const gateMs = Math.max(0, r.latency_ms.total - r.latency_ms.tier1 - tier2Elapsed);

  const nodes: Node[] = [
    { name: "State lookup", state: "ok", elapsed: r.latency_ms.state },
    tier1Dead
      ? {
          name: "Clef-flash · Tier 1",
          state: "unavailable",
          elapsed: r.latency_ms.tier1,
          detail: "unreachable — held fail-closed (SOLV-SYSTEM-003)",
        }
      : {
          name: "Clef-flash · Tier 1",
          state: "ok",
          elapsed: r.latency_ms.tier1,
          detail: r.reasons.find((reason) => reason.evidence?.source === "clef_flash")?.code,
        },
    tier2Ran
      ? ceilingHit
        ? {
            name: "SERV Reasoning · Tier 2",
            state: "unavailable",
            elapsed: r.latency_ms.tier2 ?? null,
            detail: "attempted, out of budget — held fail-closed (SOLV-SYSTEM-002)",
          }
        : {
            name: "SERV Reasoning · Tier 2",
            state: "ok",
            elapsed: r.latency_ms.tier2 ?? null,
            detail: r.reasons.find((reason) =>
              ["serv_prompt_guard", "serv_reasoning"].includes(
                reason.evidence?.source ?? "",
              ),
            )?.code,
          }
      : {
          name: "SERV Reasoning · Tier 2",
          state: "skipped",
          elapsed: null,
          detail: "skipped — this decision resolved without escalation",
        },
    {
      name: "Policy gate",
      state: "ok",
      elapsed: gateMs,
      detail: `${r.decision} · cap ${r.max_safe_spend.toFixed(2)} · total ${ms(r.latency_ms.total)}`,
    },
  ];
  return nodes;
}

const STATE_GLYPH: Record<NodeState, string> = {
  ok: "✓",
  unavailable: "✗",
  skipped: "·",
  idle: "·",
};

const STATE_CLASS: Record<NodeState, string> = {
  ok: "text-ink",
  unavailable: "text-block-ink",
  skipped: "text-muted",
  idle: "text-muted/60",
};

function PipelineRow({ node, index, live }: { node: Node; index: number; live: boolean }) {
  return (
    <li
      className={`flex items-baseline gap-3 border-b border-border py-2.5 last:border-b-0 ${
        live ? "node-resolve" : ""
      }`}
      style={live ? { animationDelay: `${index * 120}ms` } : undefined}
    >
      <span aria-hidden className={`w-4 shrink-0 font-mono ${STATE_CLASS[node.state]}`}>
        {STATE_GLYPH[node.state]}
      </span>
      <span className="min-w-0 grow">
        <span
          className={
            node.state === "skipped" || node.state === "idle"
              ? "text-muted"
              : "text-ink"
          }
        >
          {node.name}
        </span>
        {node.detail && (
          <span className="block font-mono text-xs text-muted">{node.detail}</span>
        )}
      </span>
      <span className={`font-mono text-sm tabular-nums ${STATE_CLASS[node.state]}`}>
        {ms(node.elapsed)}
      </span>
    </li>
  );
}

export function Pipeline({
  result,
  running,
}: {
  result: PreSpendCheckResponse | null;
  running: boolean;
}): JSX.Element {
  const nodes = result ? nodesFor(result) : idleNodes();
  return (
    <section aria-labelledby="pipeline-title" className="border border-border bg-surface p-5">
      <h2 id="pipeline-title" className="font-head text-xl font-bold">
        Decision pipeline
      </h2>
      <p className="mt-0.5 mb-3 text-sm text-muted">
        {running
          ? "one reflex call in flight — the clock is the budget"
          : result
            ? `receipt ${result.receipt_id}`
            : "nothing has been decided yet — run a check"}
      </p>
      {result && (
        <div className="mb-4 flex items-center gap-3 border border-border bg-bg px-3 py-2">
          <DecisionBadge decision={result.decision} />
          <span className="font-mono text-sm tabular-nums">
            {ms(result.latency_ms.total)} total
          </span>
          {/* Terracotta never carries body-size text (DESIGN §2.3): the link
              uses the darkened accent fill, 6.01:1 on the surface. */}
          <a
            href={result.receipt_url}
            className="ml-auto font-mono text-sm text-accent-fill underline decoration-accent decoration-1 underline-offset-2"
          >
            public trust receipt ↗
          </a>
        </div>
      )}
      <ol aria-label="pipeline stages">
        {nodes.map((node, i) => (
          <PipelineRow key={node.name} node={node} index={i} live={result !== null} />
        ))}
      </ol>
    </section>
  );
}
