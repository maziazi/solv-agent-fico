// Shared formatting helpers. Every numeral the desk shows is data, so every
// numeral goes through JetBrains Mono at the call site (DESIGN §3) and
// through one of these formatters — no ad-hoc string building.

export function usd(amount: number): string {
  return `$${amount.toLocaleString("en-US", {
    minimumFractionDigits: 2,
    maximumFractionDigits: 2,
  })}`;
}

export function ms(value: number | null | undefined): string {
  return value === null || value === undefined ? "—" : `${Math.round(value)}ms`;
}

export function score(value: number | null): string {
  return value === null ? "UNRATED" : String(value);
}

export function shortHash(hash: string): string {
  return hash.length <= 20 ? hash : `${hash.slice(0, 10)}…${hash.slice(-6)}`;
}

/** Address of a counterparty's detail view on the explorer. */
export function detailPath(id: string): string {
  return `/explorer?c=${encodeURIComponent(id)}`;
}
