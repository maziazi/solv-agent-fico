// Band legend (docs/FRONTEND-V2.md §6). Wherever a band chip appears without
// the table that explains it, this block is what a reader needs: the range,
// the default cap, and what the gate does.
//
// The calibration note is not decoration. A 300–850 scale borrows the look of
// a credit score, so the page has to say in its own words that the boundaries
// are chosen rather than measured (docs/TIERS-V2.md §6).

import type { JSX } from "react";

import { BANDS, bandCapLabel, bandRange } from "../bands";
import { TierBadge } from "./Badges";

export function BandLegend({
  compact = false,
}: {
  /** Drops the meaning column, for sidebars and narrow panels. */
  compact?: boolean;
}): JSX.Element {
  return (
    <section
      aria-labelledby="band-legend-title"
      className="bg-surface p-5"
    >
      <h2 id="band-legend-title" className="font-head text-xl font-bold">
        Bands and what they allow
      </h2>
      <p className="mt-1 text-sm text-muted">
        Defaults. A decision on screen always shows the cap that was actually in
        force when it was made.
      </p>

      <div className="mt-4 overflow-x-auto">
        <table className="w-full border-collapse text-left text-sm">
          <thead>
            <tr className="text-xs uppercase tracking-wider text-muted">
              <th scope="col" className="py-2 pr-4 font-medium">
                Band
              </th>
              <th scope="col" className="py-2 pr-4 font-medium">
                Score
              </th>
              <th scope="col" className="py-2 pr-4 font-medium">
                Max safe spend
              </th>
              <th scope="col" className="py-2 pr-4 font-medium">
                Default outcome
              </th>
              {!compact && (
                <th scope="col" className="py-2 font-medium">
                  What it means
                </th>
              )}
            </tr>
          </thead>
          <tbody>
            {BANDS.map((band) => (
              <tr key={band.id} className="">
                <td className="py-2.5 pr-4 align-top">
                  <TierBadge tier={band.id} />
                </td>
                <td className="py-2.5 pr-4 align-top font-mono tabular-nums">
                  {bandRange(band)}
                </td>
                <td className="py-2.5 pr-4 align-top font-mono tabular-nums">
                  {bandCapLabel(band)}
                </td>
                <td className="py-2.5 pr-4 align-top">{band.outcome}</td>
                {!compact && (
                  <td className="py-2.5 align-top text-muted">{band.meaning}</td>
                )}
              </tr>
            ))}
          </tbody>
        </table>
      </div>

      <p className="mt-4 pt-3 text-xs text-muted">
        <span className="font-mono uppercase tracking-wider">unproven</span> —
        the boundaries are the published FICO boundaries so you can check them
        against a public reference, not because Solv has measured default rates.
        It has none. The caps are conservative operator defaults. Nothing here is
        calibrated against outcomes.
      </p>
    </section>
  );
}
