// Decision and tier badges (DESIGN §2.4, §5). Colour never carries meaning
// alone: every badge carries its word, uppercase, in Space Grotesk.

import type { JSX } from "react";

export type Decision = "APPROVE" | "HOLD" | "BLOCK";

const DECISION_CLASS: Record<Decision, string> = {
  APPROVE: "bg-approve-solid text-white",
  HOLD: "bg-hold-solid text-ink",
  BLOCK: "bg-block-solid text-white",
};

export function DecisionBadge({ decision }: { decision: Decision }): JSX.Element {
  return (
    <span
      className={`inline-block w-fit rounded-md px-2 py-0.5 font-head text-sm font-medium uppercase tracking-wider ${DECISION_CLASS[decision]}`}
    >
      {decision}
    </span>
  );
}

// Band colour follows the rule in DESIGN §4.2: a band carries a decision
// colour only when it *determines* that decision. POOR always blocks and
// UNRATED always holds, so those two are coloured. The four bands that may
// approve depend on the amount, so colour would overstate them — they stay
// neutral, and their relative standing is carried by filled pips, which
// survive greyscale and colour blindness (DESIGN §7).
const BAND_CLASS: Record<string, string> = {
  POOR: "bg-block-solid text-white",
  UNRATED: "bg-hold-solid text-ink",
};

/** Pips filled per band, out of four. POOR and UNRATED show none. */
const BAND_PIPS: Record<string, number> = {
  FAIR: 1,
  GOOD: 2,
  VERY_GOOD: 3,
  EXCEPTIONAL: 4,
};

export function TierBadge({ tier }: { tier: string }): JSX.Element {
  const cls = BAND_CLASS[tier] ?? "bg-ink text-bg";
  const pips = BAND_PIPS[tier] ?? 0;
  return (
    <span
      className={`inline-flex w-fit items-center gap-1.5 rounded-md px-2 py-0.5 font-head text-xs font-medium uppercase tracking-wider ${cls}`}
    >
      {tier.replaceAll("_", "-")}
      {pips > 0 && (
        <span aria-hidden className="flex gap-px">
          {[1, 2, 3, 4].map((pip) => (
            <span
              key={pip}
              className={`inline-block h-2 w-1 ${pip <= pips ? "bg-bg" : "bg-muted"}`}
            />
          ))}
        </span>
      )}
    </span>
  );
}

export function EffectChip({ effect }: { effect: string }): JSX.Element {
  return (
    <span className="inline-block rounded-md bg-bg px-1.5 py-px font-mono text-xs text-muted">
      {effect}
    </span>
  );
}

const SEVERITY_CLASS: Record<string, string> = {
  critical: "bg-block-solid text-white",
  high: "bg-hold-solid text-ink",
  medium: "bg-surface text-ink",
  info: "bg-surface text-muted",
};

export function SeverityChip({ severity }: { severity: string }): JSX.Element {
  const cls = SEVERITY_CLASS[severity] ?? "bg-surface text-muted";
  return (
    <span className={`inline-block rounded-md px-1.5 py-px font-mono text-xs ${cls}`}>
      {severity}
    </span>
  );
}
