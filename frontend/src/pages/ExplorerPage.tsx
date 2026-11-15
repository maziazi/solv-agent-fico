// `/explorer` — the registry and receipts (docs/DESIGN.md §4.3, Frontend
// v2 renaming): the
// directory table with real tiers, the learned guardrails (shadow rules
// visibly not enforcing), and the Reppo quota meter. This page must not
// claim an accuracy improvement — there is no labelled ground-truth set yet,
// so the Attack Lab pass rate is simply not shown.

import { useEffect, useMemo, useState } from "react";
import { Link, useSearchParams } from "react-router-dom";
import type { JSX } from "react";

import { fetchDirectory, ApiError } from "../api";
import { detailPath, score, usd } from "../format";
import { DecisionBadge, TierBadge, EffectChip } from "../components/Badges";
import { CounterpartyDetail, CounterpartySearch } from "../components/CounterpartyCheck";
import type { CounterpartyEntry, DirectoryResponse, LearnedRuleEntry } from "../types";

type SortKey = "score" | "age";

function sortEntries(
  entries: CounterpartyEntry[],
  key: SortKey,
): CounterpartyEntry[] {
  const sorted = [...entries];
  // Nulls sort last in both orders: an UNRATED counterparty never jumps to
  // the top by pretending to have the smallest (or largest) score. Ties
  // return 0 so the store's id order survives the sort.
  const value = (entry: CounterpartyEntry) =>
    key === "score" ? (entry.fico_effective ?? -1) : (entry.score_age_days ?? -1);
  sorted.sort((a, b) => {
    const av = value(a);
    const bv = value(b);
    if (av === bv) return 0;
    return av < bv ? 1 : -1;
  });
  return sorted;
}

function DirectoryTable({ entries }: { entries: CounterpartyEntry[] }): JSX.Element {
  const [sortKey, setSortKey] = useState<SortKey>("score");
  const sorted = useMemo(() => sortEntries(entries, sortKey), [entries, sortKey]);

  return (
    <section aria-labelledby="directory-title" className="border border-border bg-surface p-5">
      <div className="flex flex-wrap items-baseline justify-between gap-2">
        <h2 id="directory-title" className="font-head text-xl font-bold">
          Counterparty registry
        </h2>
        <p className="text-sm text-muted">
          sort by{" "}
          <button
            type="button"
            onClick={() => setSortKey("score")}
            aria-pressed={sortKey === "score"}
            className={`font-mono underline decoration-1 underline-offset-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
              sortKey === "score" ? "text-ink" : "text-muted"
            }`}
          >
            score
          </button>{" "}
          ·{" "}
          <button
            type="button"
            onClick={() => setSortKey("age")}
            aria-pressed={sortKey === "age"}
            className={`font-mono underline decoration-1 underline-offset-2 focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-accent ${
              sortKey === "age" ? "text-ink" : "text-muted"
            }`}
          >
            evidence age
          </button>
        </p>
      </div>

      {entries.length === 0 ? (
        <p className="mt-4 border border-dashed border-border bg-bg p-4 text-sm text-muted">
          No counterparty has a settled profile yet — an entry appears once a
          Reppo evaluation settles for it (SPEC §5.3). Every check run on{" "}
          <Link to="/desk" className="underline decoration-1 underline-offset-2">
            the desk
          </Link>{" "}
          is already counted in the receipt metrics below.
        </p>
      ) : (
        <div className="mt-3 overflow-x-auto">
          <table className="w-full border-collapse">
            <thead>
              <tr>
                <th scope="col" className="border-b border-border pb-2 pr-4 text-left text-sm font-medium text-muted">
                  Counterparty
                </th>
                <th scope="col" className="border-b border-border pb-2 pr-4 text-left text-sm font-medium text-muted">
                  Tier
                </th>
                <th scope="col" aria-sort={sortKey === "score" ? "descending" : "none"} className="border-b border-border pb-2 pr-4 text-left text-sm font-medium text-muted">
                  FICO (effective)
                </th>
                <th scope="col" aria-sort={sortKey === "age" ? "ascending" : "none"} className="border-b border-border pb-2 pr-4 text-left text-sm font-medium text-muted">
                  Evidence age
                </th>
                <th scope="col" className="border-b border-border pb-2 text-left text-sm font-medium text-muted">
                  Last decision
                </th>
              </tr>
            </thead>
            <tbody>
              {sorted.map((entry) => (
                <tr key={entry.counterparty_id} className="border-b border-border last:border-b-0">
                  <th scope="row" className="py-2.5 pr-4 text-left font-medium">
                    <Link
                      to={detailPath(entry.counterparty_id)}
                      className="block underline decoration-1 underline-offset-2"
                    >
                      {entry.counterparty_name}
                    </Link>
                    <span className="block font-mono text-xs text-muted">{entry.target_url}</span>
                  </th>
                  <td className="py-2.5 pr-4">
                    <TierBadge tier={entry.tier} />
                  </td>
                  <td className="py-2.5 pr-4 font-mono tabular-nums">
                    {score(entry.fico_effective)}
                    {entry.fico_effective !== null && entry.score_age_days !== null && (
                      <span className="text-muted"> · {entry.score_age_days}d</span>
                    )}
                  </td>
                  <td className="py-2.5 pr-4 font-mono text-sm tabular-nums">
                    {entry.score_age_days === null ? "—" : `${entry.score_age_days} d`}
                  </td>
                  <td className="py-2.5">
                    {entry.last_decision ? (
                      <span className="flex flex-wrap items-center gap-2">
                        <DecisionBadge decision={entry.last_decision.decision} />
                        <span className="font-mono text-sm">{usd(entry.last_decision.amount_usd)}</span>
                        <a
                          href={entry.last_decision.receipt_url}
                          className="font-mono text-xs text-accent-fill underline decoration-accent decoration-1 underline-offset-2 dark:text-accent"
                        >
                          receipt ↗
                        </a>
                      </span>
                    ) : (
                      <span className="text-sm text-muted">no decision recorded</span>
                    )}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        </div>
      )}
    </section>
  );
}

function Guardrails({ rules }: { rules: LearnedRuleEntry[] }): JSX.Element {
  const active = rules.filter((rule) => rule.status === "active");
  const shadow = rules.filter((rule) => rule.status === "shadow");
  return (
    <section aria-labelledby="guardrails-title" className="border border-border bg-surface p-5">
      <h2 id="guardrails-title" className="font-head text-xl font-bold">
        Learned guardrails
      </h2>
      <p className="mt-0.5 mb-4 text-sm text-muted">
        Rules distilled from Reppo critiques ride the Tier-1 reflex as extra
        questions. A rule reaches <span className="font-mono text-xs">active</span> only
        through operator promotion (SPEC §5.5).
      </p>

      {rules.length === 0 ? (
        <p className="border border-dashed border-border bg-bg p-4 text-sm text-muted">
          No rules learned yet — none distilled, none promoted.
        </p>
      ) : (
        <ul className="border-t border-border">
          {[...active, ...shadow].map((rule) => (
            <li key={rule.rule_id} className="border-b border-border py-3 last:border-b-0">
              <div className="flex flex-wrap items-center gap-2">
                <span className="font-mono text-[13px] font-medium">{rule.rule_id}</span>
                {rule.topic && <EffectChip effect={rule.topic} />}
                <EffectChip effect={rule.effect} />
                <span className="font-mono text-xs text-muted">p ≥ {rule.threshold}</span>
                {rule.status === "shadow" ? (
                  <span className="border border-hold-ink bg-hold-bg px-1.5 py-px font-mono text-xs text-hold-ink">
                    shadow — not enforcing
                  </span>
                ) : (
                  <span className="border border-approve-ink bg-approve-bg px-1.5 py-px font-mono text-xs text-approve-ink">
                    active
                  </span>
                )}
              </div>
              {rule.question && (
                <p className="mt-1 text-[15px] leading-snug">{rule.question}</p>
              )}
              {rule.created_from && (
                <div className="mt-2 border border-dashed border-border bg-bg p-3">
                  <p className="font-mono text-xs text-muted">
                    born from Reppo critique · {rule.created_from.eval_id} · criterion{" "}
                    {rule.created_from.criterion}
                  </p>
                  <p className="mt-1 font-mono text-[13px] leading-relaxed break-words">
                    “{rule.created_from.critique_quote}”
                  </p>
                </div>
              )}
            </li>
          ))}
        </ul>
      )}
    </section>
  );
}

function QuotaMeter({ response }: { response: DirectoryResponse }): JSX.Element {
  const { reppo } = response;
  return (
    <section aria-labelledby="quota-title" className="border border-border bg-surface p-5">
      <h2 id="quota-title" className="font-head text-xl font-bold">
        Reppo quota
      </h2>
      {reppo.worker_attached && reppo.credits_remaining !== null ? (
        <p className="mt-2 font-mono text-3xl font-medium tabular-nums">
          {reppo.credits_remaining}
          <span className="text-lg text-muted"> / {reppo.quota_total}</span>
          <span className="ml-2 text-sm text-muted">credits</span>
        </p>
      ) : (
        <p className="mt-2 border border-dashed border-border bg-bg p-4 text-sm text-muted">
          Audit worker not attached to this instance — no credits are in use
          and there is no quota to meter. The free quota is {reppo.quota_total}{" "}
          credits; one completed evaluation charges 440 (about $0.22). A new
          Reppo account starts with a one-time 22,000.
        </p>
      )}
    </section>
  );
}

export function ExplorerPage(): JSX.Element {
  const [data, setData] = useState<DirectoryResponse | null>(null);
  const [failed, setFailed] = useState(false);
  const [params] = useSearchParams();
  const selected = params.get("c");

  useEffect(() => {
    fetchDirectory()
      .then((directory) => {
        setData(directory);
        setFailed(false);
      })
      .catch((error: unknown) => {
        setFailed(true);
        if (!(error instanceof ApiError)) throw error;
      });
  }, []);

  return (
    <div className="mx-auto max-w-[76rem] px-4 py-8 sm:px-6">
      <header className="mb-6">
        <h1 className="font-head text-3xl font-bold tracking-tight">Explorer</h1>
        <p className="mt-1 max-w-3xl text-sm text-muted">
          Every counterparty the registry has a settled score for, the
          guardrails the gate is currently carrying, and the evaluation quota
          behind them. Numbers come from real receipts — the{" "}
          <Link to="/desk" className="underline decoration-1 underline-offset-2">
            desk
          </Link>{" "}
          produces them, and each one is addressable and verifiable.
        </p>
        <p className="mt-2 max-w-3xl border-l-2 border-hold-ink bg-hold-bg/40 py-2 pl-4 text-sm">
          <span className="font-mono text-xs uppercase tracking-wider text-hold-ink">
            limitation
          </span>{" "}
          — a score attaches to a claimed name and URL, not to a verified
          identity such as a domain or a wallet, so a name can be impersonated.
          Binding is planned. Compare the URL here with the one you are about
          to pay.
        </p>
      </header>

      <div className="flex flex-col gap-6" role="status" aria-live="polite">
        {failed && (
          <p role="alert" className="border-l-4 border-hold-ink bg-hold-bg px-4 py-3 text-sm">
            The registry could not be read. Treat everything below as absent,
            not as empty — a registry you cannot read is not a registry with
            nobody in it.
          </p>
        )}
        {data && (
          <>
            {selected !== null ? (
              <CounterpartyDetail key={selected} id={selected} entries={data.counterparties} />
            ) : (
              <CounterpartySearch entries={data.counterparties} />
            )}
            <DirectoryTable entries={data.counterparties} />
            <Guardrails rules={data.learned_rules} />
            <QuotaMeter response={data} />
          </>
        )}
      </div>
    </div>
  );
}
