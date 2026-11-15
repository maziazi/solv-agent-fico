// Route table for the SPA shell. PR-11 ships the desk alone; the bureau
// directory joins as PR-13 without restructuring this file.

import { Link, Route, Routes } from "react-router-dom";
import type { JSX } from "react";

import { SiteFooter, SiteHeader } from "./components/SiteChrome";
import { DeskPage } from "./pages/DeskPage";
import { BureauPage } from "./pages/BureauPage";

export function App(): JSX.Element {
  return (
    <div className="flex min-h-screen flex-col bg-bg text-ink antialiased">
      <SiteHeader />
      <main className="grow">
        <Routes>
          <Route path="/" element={<DeskPage />} />
          <Route path="/bureau" element={<BureauPage />} />
          <Route path="*" element={<NotFound />} />
        </Routes>
      </main>
      <SiteFooter />
    </div>
  );
}

function NotFound(): JSX.Element {
  return (
    <div className="mx-auto max-w-[76rem] px-4 py-16 sm:px-6">
      <p className="font-head text-2xl font-bold">NOT FOUND</p>
      <p className="mt-2 text-muted">
        Nothing lives at this address. The desk is at{" "}
        <Link to="/" className="underline decoration-1 underline-offset-2">
          /
        </Link>{" "}
        and the bureau at{" "}
        <Link to="/bureau" className="underline decoration-1 underline-offset-2">
          /bureau
        </Link>
        .
      </p>
    </div>
  );
}
