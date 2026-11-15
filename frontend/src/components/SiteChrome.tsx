// Site shell shared by every route (docs/FRONTEND-V2.md §2).
//
// The navbar is the sitemap: every feature named in the README is reachable
// in at most two clicks from here. If something is not in this list, either
// the list is wrong or the feature does not exist yet — and a dead link is
// the worse of the two, so route stubs ship alongside the nav.
//
// The desk lives at /desk, not at /. The form is the product's demo, not its
// front door.

import { useEffect, useId, useRef, useState } from "react";
import type { JSX } from "react";
import { Link, NavLink } from "react-router-dom";

import { SolvMark } from "./Logo";

interface NavItem {
  label: string;
  to: string;
  /** Second-level links: a dropdown on desktop, inline in the drawer. */
  children?: { label: string; to: string; blurb: string }[];
}

const NAV: NavItem[] = [
  {
    label: "Product",
    to: "/how-it-works",
    children: [
      {
        label: "Pre-spend gate",
        to: "/how-it-works#gate",
        blurb: "APPROVE, HOLD or BLOCK, with reasons",
      },
      {
        label: "Counterparty registry",
        to: "/how-it-works#registry",
        blurb: "Independent evaluation, five bands, age decay",
      },
      {
        label: "Learned guardrails",
        to: "/how-it-works#rules",
        blurb: "Critiques become shadow rules",
      },
      {
        label: "Attack Lab",
        to: "/attack-lab",
        blurb: "Six attack categories, 20 cases, zero approvals",
      },
    ],
  },
  { label: "How it works", to: "/how-it-works" },
  { label: "See it work", to: "/proof" },
  { label: "Explorer", to: "/explorer" },
  { label: "Docs", to: "/docs" },
  { label: "Pricing", to: "/pricing" },
];

const linkBase =
  "rounded-md px-1 py-1 text-sm font-medium transition-colors hover:text-ink focus-visible:text-ink";

function navLinkClass({ isActive }: { isActive: boolean }): string {
  return `${linkBase} ${
    isActive
      ? "text-ink underline decoration-accent decoration-2 underline-offset-[10px]"
      : "text-muted"
  }`;
}

function ProductMenu(): JSX.Element {
  const [open, setOpen] = useState(false);
  const menuId = useId();
  const wrapper = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!open) return;
    function onPointerDown(event: PointerEvent) {
      if (!wrapper.current?.contains(event.target as Node)) setOpen(false);
    }
    function onKeyDown(event: KeyboardEvent) {
      if (event.key === "Escape") setOpen(false);
    }
    document.addEventListener("pointerdown", onPointerDown);
    document.addEventListener("keydown", onKeyDown);
    return () => {
      document.removeEventListener("pointerdown", onPointerDown);
      document.removeEventListener("keydown", onKeyDown);
    };
  }, [open]);

  const item = NAV[0];

  return (
    <div ref={wrapper} className="relative">
      <button
        type="button"
        aria-expanded={open}
        aria-controls={menuId}
        onClick={() => setOpen((was) => !was)}
        className={`${linkBase} flex items-center gap-1 text-muted`}
      >
        {item.label}
        <svg aria-hidden width="10" height="10" viewBox="0 0 10 10" fill="none">
          <path
            d="M2 3.5 5 6.5 8 3.5"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinecap="round"
            strokeLinejoin="round"
          />
        </svg>
      </button>
      {open && (
        <div
          id={menuId}
          className="absolute left-0 top-full z-20 mt-3 w-80 rounded-xl border border-border bg-surface p-2 shadow-lg shadow-ink/5"
        >
          <ul>
            {item.children?.map((child) => (
              <li key={child.to}>
                <Link
                  to={child.to}
                  onClick={() => setOpen(false)}
                  className="block rounded-lg px-3 py-2 hover:bg-wash focus-visible:bg-wash"
                >
                  <span className="text-sm font-semibold">{child.label}</span>
                  <span className="mt-0.5 block text-xs text-muted">
                    {child.blurb}
                  </span>
                </Link>
              </li>
            ))}
          </ul>
        </div>
      )}
    </div>
  );
}

const PRIMARY_BUTTON =
  "rounded-lg bg-ink px-4 py-2 text-sm font-semibold text-bg transition-opacity hover:opacity-90";

export function SiteHeader(): JSX.Element {
  const [drawerOpen, setDrawerOpen] = useState(false);
  // Closing on activation rather than on a location effect: the click is the
  // event that caused the change, so no extra render pass is needed.
  const closeDrawer = () => setDrawerOpen(false);

  return (
    <header className="sticky top-0 z-30 border-b border-border bg-bg/90 backdrop-blur">
      <div className="wrap flex items-center gap-6 py-3.5">
        <Link to="/" className="flex items-center gap-2.5" aria-label="Solv, home">
          <span className="text-accent">
            <SolvMark size={24} />
          </span>
          <span className="text-xl font-bold tracking-tight">Solv</span>
        </Link>

        <nav aria-label="main" className="ml-4 hidden items-center gap-6 lg:flex">
          <ProductMenu />
          {NAV.slice(1).map((item) => (
            <NavLink key={item.to} to={item.to} className={navLinkClass}>
              {item.label}
            </NavLink>
          ))}
        </nav>

        <div className="ml-auto flex items-center gap-3">
          <Link to="/desk" className={`hidden sm:inline-block ${PRIMARY_BUTTON}`}>
            Try the desk
          </Link>
          <button
            type="button"
            aria-expanded={drawerOpen}
            aria-controls="mobile-nav"
            onClick={() => setDrawerOpen((was) => !was)}
            className="rounded-lg border border-border bg-surface px-3 py-1.5 font-mono text-xs lg:hidden"
          >
            {drawerOpen ? "close" : "menu"}
          </button>
        </div>
      </div>

      {drawerOpen && (
        <nav
          id="mobile-nav"
          aria-label="main, mobile"
          className="border-t border-border bg-surface lg:hidden"
        >
          <ul className="wrap py-2">
            {NAV.map((item) => (
              <li
                key={item.label}
                className="border-b border-border last:border-0"
              >
                <NavLink
                  to={item.to}
                  onClick={closeDrawer}
                  className="block py-3 text-sm font-medium"
                >
                  {item.label}
                </NavLink>
                {item.children && (
                  <ul className="pb-2 pl-4">
                    {item.children.map((child) => (
                      <li key={child.to}>
                        <Link
                          to={child.to}
                          onClick={closeDrawer}
                          className="block py-1.5 text-sm text-muted"
                        >
                          {child.label}
                        </Link>
                      </li>
                    ))}
                  </ul>
                )}
              </li>
            ))}
            <li className="py-3">
              <Link
                to="/desk"
                onClick={closeDrawer}
                className={`inline-block ${PRIMARY_BUTTON}`}
              >
                Try the desk
              </Link>
            </li>
          </ul>
        </nav>
      )}
    </header>
  );
}

const FOOTER_LINKS: { label: string; to: string }[] = [
  { label: "How it works", to: "/how-it-works" },
  { label: "See it work", to: "/proof" },
  { label: "Explorer", to: "/explorer" },
  { label: "API docs", to: "/docs" },
  { label: "Pricing", to: "/pricing" },
  { label: "Attack Lab", to: "/attack-lab" },
  { label: "The desk", to: "/desk" },
];

export function SiteFooter(): JSX.Element {
  return (
    <footer className="mt-20 border-t border-border bg-wash">
      <div className="wrap py-10">
        <div className="flex items-center gap-2.5">
          <span className="text-accent">
            <SolvMark size={20} />
          </span>
          <span className="text-lg font-bold tracking-tight">Solv</span>
        </div>
        <nav aria-label="footer" className="mt-5 flex flex-wrap gap-x-6 gap-y-2">
          {FOOTER_LINKS.map((link) => (
            <Link
              key={link.to}
              to={link.to}
              className="text-sm font-medium text-muted hover:text-ink"
            >
              {link.label}
            </Link>
          ))}
        </nav>
        <p className="mt-6 max-w-2xl text-sm text-muted">
          Solv records decisions and does not move money. A receipt is a
          record, not an authorisation. Scores are not calibrated against
          outcomes.
        </p>
        <p className="mt-2 font-mono text-xs">
          <a
            href="https://github.com/maziazi/solv-agent-fico"
            className="text-accent-ink underline decoration-1 underline-offset-2"
          >
            source and specification ↗
          </a>
        </p>
      </div>
    </footer>
  );
}
