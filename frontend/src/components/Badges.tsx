// Decision and tier badges (DESIGN §2.4, §5). Colour never carries meaning
// alone: every badge carries its word, uppercase, in Space Grotesk.

import type { JSX } from "react";

export type Decision = "APPROVE" | "HOLD" | "BLOCK";

const DECISION_CLASS: Record<Decision, string> = {
  APPROVE: "bg-approve-bg text-approve-ink border-approve-ink",
  HOLD: "bg-hold-bg text-hold-ink border-hold-ink",
  BLOCK: "bg-block-bg text-block-ink border-block-ink",
};

export function DecisionBadge({ decision }: { decision: Decision }): JSX.Element {
  return (
    <span
      className={`inline-block border px-2 py-0.5 font-head text-sm font-medium uppercase tracking-wider ${DECISION_CLASS[decision]}`}
    >
      {decision}
    </span>
  );
}

// Tiers beyond the three decisions carry no semantic colour (DESIGN §4.2):
// a neutral badge never suggests an outcome the tier does not have.
const TIER_CLASS: Record<string, string> = {
  PRIME: "bg-approve-bg text-approve-ink border-approve-ink",
  HIGH_RISK: "bg-block-bg text-block-ink border-block-ink",
  UNRATED: "bg-hold-bg text-hold-ink border-hold-ink",
};

export function TierBadge({ tier }: { tier: string }): JSX.Element {
  const cls = TIER_CLASS[tier] ?? "bg-surface text-ink border-muted";
  return (
    <span
      className={`inline-block border px-2 py-0.5 font-head text-xs font-medium uppercase tracking-wider ${cls}`}
    >
      {tier.replaceAll("_", "-")}
    </span>
  );
}

export function EffectChip({ effect }: { effect: string }): JSX.Element {
  return (
    <span className="inline-block border border-border px-1.5 py-px font-mono text-xs text-muted">
      {effect}
    </span>
  );
}

const SEVERITY_CLASS: Record<string, string> = {
  critical: "bg-block-bg text-block-ink border-block-ink",
  high: "bg-hold-bg text-hold-ink border-hold-ink",
  medium: "bg-surface text-ink border-muted",
  info: "bg-surface text-muted border-border",
};

export function SeverityChip({ severity }: { severity: string }): JSX.Element {
  const cls = SEVERITY_CLASS[severity] ?? "bg-surface text-muted border-border";
  return (
    <span className={`inline-block border px-1.5 py-px font-mono text-xs ${cls}`}>
      {severity}
    </span>
  );
}
