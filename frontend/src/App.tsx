// Route table for the SPA shell (docs/FRONTEND-V2.md §2).
//
// The landing page holds `/`; the desk moved to `/desk`, because a form is a
// demo and not a front door. `/bureau` redirects rather than 404s: the word
// was confusing, but links to it already exist and breaking them would be a
// second mistake on top of the first.

import { Link, Navigate, Route, Routes } from "react-router-dom";
import type { JSX } from "react";

import { SiteFooter, SiteHeader } from "./components/SiteChrome";
import { DeskPage } from "./pages/DeskPage";
import { ExplorerPage } from "./pages/ExplorerPage";
import { LandingPage } from "./pages/LandingPage";
import {
  AttackLabPage,
  DocsPage,
  HowItWorksPage,
  PricingPage,
} from "./pages/ContentPages";

export function App(): JSX.Element {
  return (
    <div className="flex min-h-screen flex-col bg-bg text-ink antialiased">
      <SiteHeader />
      <main className="grow">
        <Routes>
          <Route path="/" element={<LandingPage />} />
          <Route path="/desk" element={<DeskPage />} />
          <Route path="/explorer" element={<ExplorerPage />} />
          <Route path="/how-it-works" element={<HowItWorksPage />} />
          <Route path="/docs" element={<DocsPage />} />
          <Route path="/pricing" element={<PricingPage />} />
          <Route path="/attack-lab" element={<AttackLabPage />} />
          {/* Superseded address, kept alive. */}
          <Route path="/bureau" element={<Navigate to="/explorer" replace />} />
          <Route path="*" element={<NotFound />} />
        </Routes>
      </main>
      <SiteFooter />
    </div>
  );
}

function NotFound(): JSX.Element {
  return (
    <div className="wrap py-16">
      <p className="font-head text-2xl font-bold">NOT FOUND</p>
      <p className="mt-2 max-w-xl text-muted">
        Nothing lives at this address. The desk is at{" "}
        <Link to="/desk" className="underline decoration-1 underline-offset-2">
          /desk
        </Link>
        , the registry at{" "}
        <Link
          to="/explorer"
          className="underline decoration-1 underline-offset-2"
        >
          /explorer
        </Link>
        .
      </p>
    </div>
  );
}
