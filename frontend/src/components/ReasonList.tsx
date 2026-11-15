// Reasons (DESIGN §4.2, SPEC §2.2): every decision shows its reasons, and an
// APPROVE is explained with the same care as a BLOCK. Evidence quotes are
// attacker-controlled text: React escapes them at render time and the block
// is labelled as untrusted, never interpreted as markup.

import type { JSX } from "react";

import type { Reason } from "../types";
import { EffectChip, SeverityChip } from "./Badges";

function EvidenceBlock({ reason }: { reason: Reason }): JSX.Element | null {
  const evidence = reason.evidence;
  if (!evidence) return null;
  return (
    <div className="mt-2 border border-dashed border-border bg-bg p-3">
      <p className="font-mono text-xs text-muted">
        untrusted counterparty text · source {evidence.source} · field{" "}
        {evidence.field}
        {evidence.locator && ` · ${evidence.locator}`}
      </p>
      {evidence.quote && (
        <p className="mt-1 font-mono text-[13px] leading-relaxed break-words whitespace-pre-wrap">
          “{evidence.quote}”
        </p>
      )}
    </div>
  );
}

export function ReasonList({ reasons }: { reasons: Reason[] }): JSX.Element {
  return (
    <ol aria-label="reasons" className="border-border">
      {reasons.map((reason, i) => (
        <li
          key={`${reason.code}-${i}`}
          className="border-b border-border py-3 first:pt-0 last:border-b-0 last:pb-0"
        >
          <div className="flex flex-wrap items-center gap-2">
            <span className="font-mono text-[13px] font-medium">{reason.code}</span>
            <SeverityChip severity={reason.severity} />
            <EffectChip effect={reason.effect} />
          </div>
          <p className="mt-1 text-[15px] leading-snug">{reason.message}</p>
          <EvidenceBlock reason={reason} />
        </li>
      ))}
    </ol>
  );
}
