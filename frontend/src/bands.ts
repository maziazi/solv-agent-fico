// The five underwriting bands and the UNRATED state (docs/TIERS-V2.md §2).
//
// One source for every surface that shows a band: the chip, the gauge ticks,
// the legend, and the explorer tables. The boundaries are the published FICO
// band boundaries so a reader can check them against a public reference —
// they are not calibrated against outcomes, and the legend says so rather
// than letting the familiar 300–850 range imply otherwise.
//
// Caps mirror the Rust defaults (`score::Caps`), but they are operator
// configurable: wherever a real decision is on screen, that decision's own
// `max_safe_spend` is the truth and these are only the defaults.

export const TRACK_MIN = 300;
export const TRACK_MAX = 850;

export type BandId =
  | "POOR"
  | "FAIR"
  | "GOOD"
  | "VERY_GOOD"
  | "EXCEPTIONAL"
  | "UNRATED";

export interface Band {
  id: BandId;
  /** Inclusive floor; `null` for UNRATED, which has no score at all. */
  floor: number | null;
  /** Inclusive ceiling; `null` for UNRATED. */
  ceiling: number | null;
  /** Short zone name used in the legend, never instead of the band id. */
  zone: string;
  /** Default cap in USD. `null` means "the operator's absolute ceiling". */
  defaultCapUsd: number | null;
  /** What the gate does by default, in the gate's own vocabulary. */
  outcome: string;
  /** One line a non-expert can act on. */
  meaning: string;
}

/** Highest band first: the order the legend and the explorer sort by. */
export const BANDS: readonly Band[] = [
  {
    id: "EXCEPTIONAL",
    floor: 800,
    ceiling: 850,
    zone: "Priority",
    defaultCapUsd: null,
    outcome: "APPROVE up to the operator ceiling",
    meaning:
      "Complete, consistent public evidence. The band cap rises to meet the operator's hard limit — it never removes it.",
  },
  {
    id: "VERY_GOOD",
    floor: 740,
    ceiling: 799,
    zone: "Trusted",
    defaultCapUsd: 250,
    outcome: "APPROVE up to the cap",
    meaning: "A consistent record across several settled evaluations.",
  },
  {
    id: "GOOD",
    floor: 670,
    ceiling: 739,
    zone: "Standard",
    defaultCapUsd: 50,
    outcome: "APPROVE up to the cap",
    meaning: "Verified live and safe, with evidence that holds up.",
  },
  {
    id: "FAIR",
    floor: 580,
    ceiling: 669,
    zone: "Watch",
    defaultCapUsd: 5,
    outcome: "APPROVE under the cap, HOLD above it",
    meaning:
      "Thin or mixed evidence. Micro-payments pass; anything larger waits for a human or a fresher evaluation.",
  },
  {
    id: "POOR",
    floor: 300,
    ceiling: 579,
    zone: "Red",
    defaultCapUsd: 0,
    outcome: "BLOCK",
    meaning:
      "Evidence of a problem, or too little evidence to pay anyone. No amount is approved.",
  },
  {
    id: "UNRATED",
    floor: null,
    ceiling: null,
    zone: "Cold start",
    defaultCapUsd: 0,
    outcome: "HOLD",
    meaning:
      "Nobody has evaluated this counterparty. That is an absence of evidence, not weak evidence — so there is no number, and no amount is approved.",
  },
] as const;

/** Band floors that get a tick on the gauge, lowest first. */
export const BAND_FLOORS: readonly number[] = [580, 670, 740, 800];

/** A band the frontend does not recognise renders as unknown, never as a guess. */
export const UNKNOWN_BAND: Band = {
  id: "UNRATED",
  floor: null,
  ceiling: null,
  zone: "Unknown",
  defaultCapUsd: 0,
  outcome: "—",
  meaning: "This build does not recognise the band the service returned.",
};

export function bandById(id: string): Band {
  return BANDS.find((band) => band.id === id) ?? UNKNOWN_BAND;
}

export function bandFor(ficoEffective: number | null): Band {
  if (ficoEffective === null) return bandById("UNRATED");
  const band = BANDS.find(
    (candidate) => candidate.floor !== null && ficoEffective >= candidate.floor,
  );
  return band ?? bandById("POOR");
}

export function isKnownBand(id: string): boolean {
  return BANDS.some((band) => band.id === id);
}

/** Band range as it appears in the legend: "740–799", or "no score". */
export function bandRange(band: Band): string {
  if (band.floor === null || band.ceiling === null) return "no score";
  return `${band.floor}–${band.ceiling}`;
}

/** Default cap as text. The top band tracks the operator ceiling. */
export function bandCapLabel(band: Band): string {
  if (band.defaultCapUsd === null) return "operator ceiling";
  return `$${band.defaultCapUsd.toFixed(2)}`;
}
