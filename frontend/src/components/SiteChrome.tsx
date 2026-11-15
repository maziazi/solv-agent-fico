// Site shell shared by every SPA route (DESIGN §4): a header that names the
// product, a footer that repeats the one sentence a visitor must not forget.
// The desk links forward — no navigation menu beyond what exists.

import type { JSX } from "react";
import { Link } from "react-router-dom";

export function SiteHeader(): JSX.Element {
  return (
    <header className="border-b border-border">
      <div className="mx-auto flex max-w-[76rem] flex-wrap items-baseline gap-x-4 gap-y-1 px-4 py-4 sm:px-6">
        <Link to="/" className="flex items-baseline gap-2">
          <span aria-hidden className="inline-block h-3 w-3 bg-accent-fill" />
          <span className="font-head text-2xl font-bold tracking-tight">Solv</span>
        </Link>
        <p className="text-sm text-muted">
          pre-spend gate & credit bureau for AI agents
        </p>
      </div>
    </header>
  );
}

export function SiteFooter(): JSX.Element {
  return (
    <footer className="mt-10 border-t border-border">
      <div className="mx-auto max-w-[76rem] px-4 py-6 text-sm text-muted sm:px-6">
        <p>
          Solv records decisions; it does not move money. A receipt is a record,
          not an authorisation.
        </p>
        <p className="mt-1 font-mono text-xs">
          <a
            href="https://github.com/maziazi/solv-agent-fico"
            className="underline decoration-1 underline-offset-2"
          >
            source & specification ↗
          </a>
        </p>
      </div>
    </footer>
  );
}
