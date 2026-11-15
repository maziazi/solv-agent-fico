// The five score bands on one strip: a proportional track on top, one card per
// band below with its range, default spending cap and what the gate does.
// Data comes from bands.ts, the same source the explorer and legend use. The
// boundaries are the published FICO ones; they are not calibrated to outcomes.

import type { JSX } from "react";

import { BANDS, TRACK_MAX, TRACK_MIN, bandCapLabel, bandRange } from "../bands";

// A five-step scale needs five distinguishable fills. They are solid hex
// values (the accent mixed into the page colour at 15/35/55/75 per cent,
// computed once), not opacity classes. This is the one place steps of one hue
// are intended: it is a data scale, not a card.
const TINT: Record<string, string> = {
  POOR: "bg-[#f5dfd5]",
  FAIR: "bg-[#efbea8]",
  GOOD: "bg-[#e99d7c]",
  VERY_GOOD: "bg-[#e37c50]",
  EXCEPTIONAL: "bg-accent",
};

function label(id: string): string {
  return id.replace("_", " ").toLowerCase().replace(/^./, (c) => c.toUpperCase());
}

export function BandStrip(): JSX.Element {
  const scored = [...BANDS].filter((band) => band.floor !== null).reverse();
  const unrated = BANDS.find((band) => band.id === "UNRATED");
  const span = TRACK_MAX - TRACK_MIN + 1;

  return (
    <div>
      <div
        role="img"
        aria-label="Score track from 300 to 850 in five bands"
        className="flex h-3 w-full overflow-hidden rounded-full"
      >
        {scored.map((band) => {
          const size = (band.ceiling as number) - (band.floor as number) + 1;
          return (
            <span
              key={band.id}
              className={TINT[band.id]}
              style={{ width: `${(size / span) * 100}%` }}
            />
          );
        })}
      </div>
      <div className="mt-1 flex justify-between font-mono text-xs text-muted">
        <span>{TRACK_MIN}</span>
        <span>{TRACK_MAX}</span>
      </div>

      <ul className="mt-5 grid gap-3 sm:grid-cols-2 lg:grid-cols-5">
        {scored.map((band) => (
          <li
            key={band.id}
            className="rounded-xl bg-surface p-4"
          >
            <span
              aria-hidden
              className={`mb-3 block h-1.5 w-10 rounded-full ${TINT[band.id]}`}
            />
            <p className="font-semibold">{label(band.id)}</p>
            <p className="font-mono text-sm text-muted">{bandRange(band)}</p>
            <p className="mt-3 text-xs uppercase tracking-wide text-muted">Default cap</p>
            <p className="font-mono text-base font-medium">{bandCapLabel(band)}</p>
          </li>
        ))}
      </ul>

      {unrated && (
        <p className="mt-4 rounded-xl bg-surface px-4 py-3 text-sm text-muted">
          <span className="font-semibold text-ink">Unrated</span> has no number
          at all. Nobody has evaluated it, so the request holds for a human.
        </p>
      )}
    </div>
  );
}
