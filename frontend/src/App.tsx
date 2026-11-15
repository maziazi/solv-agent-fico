// Route table for the SPA shell. PR-11 ships the desk alone; the bureau
// directory joins as PR-13 without restructuring this file.

import { Route, Routes } from "react-router-dom";
import type { JSX } from "react";

import { SiteFooter, SiteHeader } from "./components/SiteChrome";
import { DeskPage } from "./pages/DeskPage";

export function App(): JSX.Element {
  return (
    <div className="flex min-h-screen flex-col bg-bg text-ink antialiased">
      <SiteHeader />
      <main className="grow">
        <Routes>
          <Route path="/" element={<DeskPage />} />
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
        This desk has one page. The bureau directory arrives with the next
        milestone.
      </p>
    </div>
  );
}
