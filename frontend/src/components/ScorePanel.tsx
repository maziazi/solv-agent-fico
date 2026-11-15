// Score panel (DESIGN §4.2): an honest FICO gauge. A null score renders the
// word UNRATED — the needle never rests at 300 to represent "unknown" — and
// the gauge always has a text equivalent next to it (DESIGN §7).

import type { JSX } from "react";

import type { Pillars, ScoreSource } from "../types";
import { score } from "../format";
import { TierBadge } from "./Badges";

const TRACK_MIN = 300;
const TRACK_MAX = 850;
const TIER_FLOORS = [550, 640, 720];

function pct(value: number): number {
  return ((value - TRACK_MIN) / (TRACK_MAX - TRACK_MIN)) * 100;
}

function Gauge({ fico, raw, decay }: { fico: number; raw: number | null; decay: number | null }) {
  const fill = Math.min(Math.max(pct(fico), 0), 100);
  return (
    <div
      role="img"
      aria-label={`FICO gauge: effective ${fico} of 850, scale 300 to 850`}
      className="mt-3"
    >
      <div className="relative h-3 border border-border bg-bg">
        <div className="absolute inset-y-0 left-0 bg-ink" style={{ width: `${fill}%` }} />
        {TIER_FLOORS.map((floor) => (
          <div
            key={floor}
            aria-hidden
            className="absolute inset-y-0 w-px bg-accent"
            style={{ left: `${pct(floor)}%` }}
          />
        ))}
        {/* Raw score marker: a dashed hairline the fill has decayed away from. */}
        {raw !== null && decay !== null && decay > 0 && raw !== fico && (
          <div
            aria-hidden
            className="absolute inset-y-0 w-0 border-l-2 border-dashed border-accent"
            style={{ left: `${pct(raw)}%` }}
          />
        )}
      </div>
      <div className="mt-1 flex justify-between font-mono text-xs text-muted">
        <span>300</span>
        <span>550</span>
        <span>640</span>
        <span>720</span>
        <span>850</span>
      </div>
    </div>
  );
}

function PillarBar({ label, value }: { label: string; value: number }) {
  return (
    <div>
      <div className="flex items-baseline justify-between">
        <span className="text-sm text-muted">{label}</span>
        <span className="font-mono text-sm tabular-nums">{value}/10</span>
      </div>
      <div className="mt-1 h-1.5 w-full border border-border bg-bg">
        <div className="h-full bg-ink" style={{ width: `${value * 10}%` }} />
      </div>
    </div>
  );
}

export function ScorePanel({
  tier,
  fico,
  raw,
  decay,
  ageDays,
  pillars,
  source,
}: {
  tier: string;
  fico: number | null;
  raw: number | null;
  decay: number | null;
  ageDays: number | null;
  pillars: Pillars | null;
  source: ScoreSource | null;
}): JSX.Element {
  return (
    <section aria-labelledby="score-title" className="border border-border bg-surface p-5">
      <div className="flex items-center justify-between">
        <h2 id="score-title" className="font-head text-xl font-bold">
          Score
        </h2>
        <TierBadge tier={tier} />
      </div>

      {fico === null ? (
        <>
          <p className="mt-2 font-mono text-4xl font-medium tracking-tight">UNRATED</p>
          <p className="mt-1 text-sm text-muted">
            No settled evaluation backs this decision, so there is no score to show.
          </p>
        </>
      ) : (
        <>
          <p className="mt-2 font-mono text-4xl font-medium tracking-tight tabular-nums">
            {score(fico)}
            <span className="text-lg text-muted"> / 850</span>
          </p>
          <Gauge fico={fico} raw={raw} decay={decay} />
          <p className="mt-2 text-sm text-muted">
            Effective score.
            {raw !== null && ` Raw ${raw}`}
            {decay !== null && decay > 0 && `, decay −${decay}`}
            {ageDays !== null && ` Evidence ${ageDays} day${ageDays === 1 ? "" : "s"} old.`}
          </p>
        </>
      )}

      {pillars && (
        <div className="mt-4 grid grid-cols-2 gap-x-5 gap-y-3 border-t border-border pt-4">
          <PillarBar label="reliability" value={pillars.reliability} />
          <PillarBar label="track record" value={pillars.track_record} />
          <PillarBar label="safety" value={pillars.safety} />
          <PillarBar label="maturity" value={pillars.maturity} />
        </div>
      )}

      {source && (
        <p className="mt-4 border-t border-border pt-3 font-mono text-xs text-muted">
          score from {source.provider} · {source.eval_id} · settled {source.settled_at}
          {source.peer_assessment_count !== undefined &&
            ` · ${source.peer_assessment_count} peer assessment${
              source.peer_assessment_count === 1 ? "" : "s"
            }`}
        </p>
      )}
    </section>
  );
}
