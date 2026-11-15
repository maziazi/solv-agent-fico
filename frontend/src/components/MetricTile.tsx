// Hero metric tiles (DESIGN §4.1, §5). A metric with no data renders "—" with
// the words "no data" — never a placeholder number, never a constant dressed
// up as a measurement.

import type { JSX } from "react";

interface MetricTileProps {
  label: string;
  value: string;
  unit?: string;
  verification: "MEASURED" | "CITED" | "UNPROVEN";
  detail: string;
}

const VERIFICATION_CLASS: Record<MetricTileProps["verification"], string> = {
  MEASURED: "border-approve-ink text-approve-ink",
  CITED: "border-muted text-muted",
  UNPROVEN: "border-hold-ink text-hold-ink",
};

export function MetricTile({
  label,
  value,
  unit,
  verification,
  detail,
}: MetricTileProps): JSX.Element {
  const empty = value === "—";
  return (
    <div className="bg-surface p-4">
      <p className="text-sm text-muted">{label}</p>
      <p className="mt-1 font-mono text-3xl font-medium tracking-tight tabular-nums">
        {value}
        {unit && <span className="ml-1 text-base text-muted">{unit}</span>}
      </p>
      <p className="mt-2 flex items-baseline gap-2">
        <span
          className={`inline-block border px-1 font-mono text-[10px] tracking-wider ${VERIFICATION_CLASS[verification]}`}
        >
          {verification}
        </span>
        <span className="text-xs text-muted">{empty ? "no data" : detail}</span>
      </p>
    </div>
  );
}
