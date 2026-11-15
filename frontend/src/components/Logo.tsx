// The Solv mark (docs/LOGO.md).
//
// A gate seen head-on: two facing posts with a gap between them. The posts
// are the two parties; the gap is the decision. It reads second as a bracket
// pair, which is the right secondary meaning for a tool developers put in
// front of a payment call.
//
// Drawn on a 24×24 grid in whole units — block weight 6, gap 4, arm 10 — so
// every edge lands on a pixel boundary at 16, 24, 32, 48, and 64 px. No
// curves, no strokes, no gradients: six rectangles in one colour, which is
// what keeps it legible at favicon size and faithful to DESIGN §1.
//
// Colour comes from `currentColor`, so the mark takes the ink of whatever it
// sits in and needs no separate dark-theme asset.

import type { JSX } from "react";

const GRID = 24;
/** Block weight. A quarter of the grid: heavy enough to survive 16 px. */
const W = 6;
/** The opening between the posts. */
const GAP = 4;
/** Horizontal reach of the top and bottom arms. */
const ARM = (GRID - GAP) / 2;
/** Left edge of the right-hand post's arms. */
const RIGHT = ARM + GAP;

export type MarkVariant = "open" | "sealed";

export function SolvMark({
  size = 24,
  variant = "open",
  title,
}: {
  size?: number;
  /**
   * `open` is the identity mark. `sealed` drops a block into the gap — the
   * decision, recorded — and is for display sizes only: at 16 px the block
   * closes the opening and the gate stops reading as a gate.
   */
  variant?: MarkVariant;
  /** Omit for decorative use; the mark is then hidden from assistive tech. */
  title?: string;
}): JSX.Element {
  return (
    <svg
      viewBox={`0 0 ${GRID} ${GRID}`}
      width={size}
      height={size}
      fill="currentColor"
      shapeRendering="crispEdges"
      role={title ? "img" : undefined}
      aria-label={title}
      aria-hidden={title ? undefined : true}
      focusable="false"
    >
      {/* Left post */}
      <rect x="0" y="0" width={ARM} height={W} />
      <rect x="0" y="0" width={W} height={GRID} />
      <rect x="0" y={GRID - W} width={ARM} height={W} />
      {/* Right post */}
      <rect x={RIGHT} y="0" width={ARM} height={W} />
      <rect x={GRID - W} y="0" width={W} height={GRID} />
      <rect x={RIGHT} y={GRID - W} width={ARM} height={W} />
      {/* The decision, held in the opening. */}
      {variant === "sealed" && (
        <rect x={ARM} y={(GRID - GAP) / 2} width={GAP} height={GAP} />
      )}
    </svg>
  );
}

export function SolvLockup({
  size = 24,
  tagline = false,
}: {
  size?: number;
  tagline?: boolean;
}): JSX.Element {
  return (
    <span className="inline-flex items-center gap-2.5">
      <SolvMark size={size} />
      <span
        className="font-head font-bold tracking-tight"
        style={{ fontSize: size * 0.95 }}
      >
        Solv
      </span>
      {tagline && (
        <span className="text-sm text-muted">
          pre-spend gate &amp; counterparty registry
        </span>
      )}
    </span>
  );
}
