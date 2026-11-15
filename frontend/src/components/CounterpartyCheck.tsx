// Counterparty check: a search over the directory and a per-counterparty
// view. Everything here is read from GET /api/v1/directory and, for the
// pillars and score source, the counterparty's last receipt. UNRATED is shown
// as an absence of evidence, never as a weak score.

import { useEffect, useMemo, useState } from "react";
import { Link } from "react-router-dom";
import type { JSX } from "react";

import { ApiError, fetchReceipt } from "../api";
import { bandCapLabel, bandFor } from "../bands";
import { detailPath, score, usd } from "../format";
import type { CounterpartyEntry, ReceiptResponse } from "../types";
import { DecisionBadge, TierBadge } from "./Badges";

/** Days a score lives before it is discarded (SPEC §4.4). */
const SCORE_LIFETIME_DAYS = 90;

function matches(entry: CounterpartyEntry, query: string): boolean {
  return [entry.counterparty_id, entry.counterparty_name, entry.target_url].some(
    (field) => field.toLowerCase().includes(query),
  );
}

function Unrated({ what }: { what: string }): JSX.Element {
  return (
    <div className="border border-dashed border-border bg-bg p-4 text-sm">
      <p className="font-medium">
        <span className="font-mono">UNRATED</span>: no evidence on file for “{what}”.
      </p>
      <p className="mt-1 text-muted">
        That is an absence of evidence, not bad evidence. Solv holds no score
        for it, so no amount is pre-approved and a payment would hold for a
        human.
      </p>
      <p className="mt-2 text-muted">
        <Link to="/desk" className="underline decoration-1 underline-offset-2">
          Run a check on the desk
        </Link>{" "}
        to see what the gate says today, or read{" "}
        <Link to="/docs" className="underline decoration-1 underline-offset-2">
          how a counterparty gets rated
        </Link>
        : a rating exists only after an independent evaluation completes.
      </p>
    </div>
  );
}

export function CounterpartySearch({
  entries,
}: {
  entries: CounterpartyEntry[];
}): JSX.Element {
  const [query, setQuery] = useState("");
  const needle = query.trim().toLowerCase();
  const found = useMemo(
    () => (needle === "" ? [] : entries.filter((entry) => matches(entry, needle))),
    [entries, needle],
  );

  return (
    <section aria-labelledby="check-title" className="border border-border bg-surface p-5">
      <h2 id="check-title" className="font-head text-xl font-bold">
        Check a counterparty
      </h2>
      <p className="mt-1 text-sm text-muted">
        Type a name or a URL. Results come from the registry on this instance;
        a name that is not listed has no rating.
      </p>
      <label htmlFor="check-query" className="mt-4 block text-sm font-medium">
        Name or URL
      </label>
      <input
        id="check-query"
        type="search"
        value={query}
        onChange={(event) => setQuery(event.target.value)}
        placeholder="e.g. github-rest-api or https://api.github.com"
        autoComplete="off"
        className="mt-1 w-full border border-border bg-bg px-3 py-2 font-mono text-sm focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent"
      />
      <div className="mt-4" aria-live="polite">
        {needle !== "" && found.length === 0 && <Unrated what={query.trim()} />}
        {found.length > 0 && (
          <ul className="divide-y divide-border border border-border">
            {found.map((entry) => (
              <li key={entry.counterparty_id} className="flex flex-wrap items-center gap-3 px-3 py-2.5">
                <Link
                  to={detailPath(entry.counterparty_id)}
                  className="font-medium underline decoration-1 underline-offset-2"
                >
                  {entry.counterparty_name}
                </Link>
                <span className="font-mono text-xs text-muted">{entry.target_url}</span>
                <TierBadge tier={entry.tier} />
                <span className="font-mono text-sm tabular-nums">
                  {score(entry.fico_effective)}
                </span>
              </li>
            ))}
          </ul>
        )}
      </div>
    </section>
  );
}

function Fact({ label, children }: { label: string; children: React.ReactNode }): JSX.Element {
  return (
    <div>
      <dt className="text-sm text-muted">{label}</dt>
      <dd className="mt-0.5 font-mono text-sm tabular-nums">{children}</dd>
    </div>
  );
}

export function CounterpartyDetail({
  id,
  entries,
}: {
  id: string;
  entries: CounterpartyEntry[];
}): JSX.Element {
  const entry = entries.find((candidate) => candidate.counterparty_id === id);
  const lastReceiptId = entry?.last_decision?.receipt_id ?? null;
  const [receipt, setReceipt] = useState<ReceiptResponse | null>(null);
  const [receiptFailed, setReceiptFailed] = useState(false);

  useEffect(() => {
    if (lastReceiptId === null) return;
    fetchReceipt(lastReceiptId)
      .then(setReceipt)
      .catch((error: unknown) => {
        setReceiptFailed(true);
        if (!(error instanceof ApiError)) throw error;
      });
  }, [lastReceiptId]);

  return (
    <section aria-labelledby="detail-title" className="border border-border bg-surface p-5">
      <p className="text-sm">
        <Link to="/explorer" className="underline decoration-1 underline-offset-2">
          ← back to the registry
        </Link>
      </p>
      {entry === undefined ? (
        <div className="mt-4">
          <h2 id="detail-title" className="font-head text-xl font-bold">
            {id}
          </h2>
          <div className="mt-3">
            <Unrated what={id} />
          </div>
        </div>
      ) : (
        <Rated entry={entry} receipt={receipt} receiptFailed={receiptFailed} />
      )}
    </section>
  );
}

function Rated({
  entry,
  receipt,
  receiptFailed,
}: {
  entry: CounterpartyEntry;
  receipt: ReceiptResponse | null;
  receiptFailed: boolean;
}): JSX.Element {
  const band = bandFor(entry.fico_effective);
  const age = entry.score_age_days;
  const daysLeft = age === null ? null : Math.max(0, SCORE_LIFETIME_DAYS - age);
  const pillars = receipt?.pillars ?? null;
  const source = receipt?.score_source ?? null;

  return (
    <div className="mt-4">
      <h2 id="detail-title" className="font-head text-xl font-bold">
        {entry.counterparty_name}
      </h2>
      <p className="font-mono text-xs text-muted">{entry.target_url}</p>
      <p className="mt-1 text-xs text-muted">
        Scored under this claimed name and URL; not bound to a verified
        identity yet, so confirm the URL is the one you are about to pay.
      </p>

      {entry.fico_effective === null ? (
        <div className="mt-4">
          <Unrated what={entry.counterparty_name} />
        </div>
      ) : (
        <dl className="mt-4 grid grid-cols-2 gap-4 sm:grid-cols-4">
          <Fact label="Band">
            <TierBadge tier={entry.tier} />
          </Fact>
          <Fact label="Score (effective)">{score(entry.fico_effective)}</Fact>
          <Fact label="Default cap for this band">{bandCapLabel(band)}</Fact>
          <Fact label="Score age / expires">
            {age === null || daysLeft === null ? "—" : `${age} d / in ${daysLeft} d`}
          </Fact>
        </dl>
      )}

      <h3 className="mt-6 font-head text-lg font-bold">Evidence</h3>
      {entry.last_decision === null ? (
        <p className="mt-1 text-sm text-muted">
          No decision has been recorded for this counterparty, so there is no
          receipt to read the score source from.
        </p>
      ) : receiptFailed ? (
        <p role="alert" className="mt-1 text-sm">
          The last receipt could not be read. Treat the source and pillars as
          absent, not as empty.
        </p>
      ) : receipt === null ? (
        <p className="mt-1 text-sm text-muted">Loading the last receipt…</p>
      ) : (
        <dl className="mt-2 grid grid-cols-1 gap-4 sm:grid-cols-2">
          <Fact label="Source evaluation">
            {source ? `${source.provider} · ${source.eval_id}` : "none"}
          </Fact>
          <Fact label="Cap at the last check">{usd(receipt.max_safe_spend)}</Fact>
          {pillars && (
            <>
              <Fact label="Reliability / Track record">
                {pillars.reliability} / {pillars.track_record} of 10
              </Fact>
              <Fact label="Safety / Maturity">
                {pillars.safety} / {pillars.maturity} of 10
              </Fact>
            </>
          )}
        </dl>
      )}
      {pillars && (
        <p className="mt-2 text-xs text-muted">
          Only the track-record pillar is filled from the evaluator today; the
          other three sit at a neutral 5.
        </p>
      )}

      {entry.last_decision && (
        <p className="mt-4 flex flex-wrap items-center gap-2 text-sm">
          <span className="text-muted">Last decision</span>
          <DecisionBadge decision={entry.last_decision.decision} />
          <span className="font-mono">{usd(entry.last_decision.amount_usd)}</span>
          <a
            href={entry.last_decision.receipt_url}
            className="font-mono text-xs text-accent-ink underline decoration-accent decoration-1 underline-offset-2"
          >
            receipt ↗
          </a>
        </p>
      )}
    </div>
  );
}
