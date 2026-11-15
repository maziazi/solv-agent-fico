// Small line icons for the landing diagram. Plain SVG, `currentColor`, no
// external assets (the CSP allows only same-origin and data: images).
// They are generic marks, not the providers' logos.

import type { JSX } from "react";

interface IconProps {
  size?: number;
  className?: string;
}

function Svg({
  size = 28,
  className,
  children,
}: IconProps & { children: JSX.Element | JSX.Element[] }): JSX.Element {
  return (
    <svg
      aria-hidden
      className={className}
      width={size}
      height={size}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.6"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      {children}
    </svg>
  );
}

/** An AI agent: a head with two eyes and an antenna. */
export function IconAgent(props: IconProps): JSX.Element {
  return (
    <Svg {...props}>
      <rect x="4" y="8" width="16" height="11" rx="3" />
      <path d="M12 8V5" />
      <circle cx="12" cy="4" r="1" />
      <circle cx="9" cy="13" r="1" fill="currentColor" />
      <circle cx="15" cy="13" r="1" fill="currentColor" />
    </Svg>
  );
}

/** Cloudflare Workers AI: a cloud. */
export function IconCloud(props: IconProps): JSX.Element {
  return (
    <Svg {...props}>
      <path d="M7 18a4 4 0 0 1-.6-7.96A5.5 5.5 0 0 1 17 8.5a4.75 4.75 0 0 1 .5 9.5H7Z" />
    </Svg>
  );
}

/** OpenServ SERV: connected reasoning nodes. */
export function IconNodes(props: IconProps): JSX.Element {
  return (
    <Svg {...props}>
      <circle cx="12" cy="5" r="2" />
      <circle cx="5" cy="18" r="2" />
      <circle cx="19" cy="18" r="2" />
      <path d="M11 7 6 16M13 7l5 9M7 18h10" />
    </Svg>
  );
}

/** The policy gate: a shield with a check. */
export function IconGate(props: IconProps): JSX.Element {
  return (
    <Svg {...props}>
      <path d="M12 3 5 6v5c0 4.5 2.9 8.2 7 10 4.1-1.8 7-5.5 7-10V6l-7-3Z" />
      <path d="m9 12 2.2 2.2L15.5 10" />
    </Svg>
  );
}

/** A receipt: a page with lines. */
export function IconReceipt(props: IconProps): JSX.Element {
  return (
    <Svg {...props}>
      <path d="M6 3h12v18l-3-2-3 2-3-2-3 2V3Z" />
      <path d="M9 8h6M9 12h6" />
    </Svg>
  );
}

/** Reppo Eval Router: a balance, evaluators weighing a claim. */
export function IconScale(props: IconProps): JSX.Element {
  return (
    <Svg {...props}>
      <path d="M12 4v16M8 20h8M5 7h14" />
      <path d="M5 7 2.5 13a3 3 0 0 0 5 0L5 7ZM19 7l-2.5 6a3 3 0 0 0 5 0L19 7Z" />
    </Svg>
  );
}

/** A queued audit request: a list with a plus. */
export function IconQueue(props: IconProps): JSX.Element {
  return (
    <Svg {...props}>
      <path d="M4 6h11M4 11h8M4 16h6" />
      <path d="M18 13v6M15 16h6" />
    </Svg>
  );
}

/** A stored score profile: a card with a gauge bar. */
export function IconProfile(props: IconProps): JSX.Element {
  return (
    <Svg {...props}>
      <rect x="3" y="5" width="18" height="14" rx="2.5" />
      <path d="M7 15h6" />
      <circle cx="8" cy="10" r="1.6" />
      <path d="M13 10h4" />
    </Svg>
  );
}

/** A confidence filter: a funnel. */
export function IconFilter(props: IconProps): JSX.Element {
  return (
    <Svg {...props}>
      <path d="M4 5h16l-6 7v6l-4 2v-8L4 5Z" />
    </Svg>
  );
}

export function ArrowRight({ className }: { className?: string }): JSX.Element {
  return (
    <svg
      aria-hidden
      className={className}
      width="20"
      height="20"
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M4 10h11M11 5l5 5-5 5" />
    </svg>
  );
}

export function ArrowDown({ className }: { className?: string }): JSX.Element {
  return (
    <svg
      aria-hidden
      className={className}
      width="20"
      height="20"
      viewBox="0 0 20 20"
      fill="none"
      stroke="currentColor"
      strokeWidth="1.8"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <path d="M10 3v11M5 10l5 5 5-5" />
    </svg>
  );
}
