// Shapes of the Solv API, mirrored from the Rust response structs
// (docs/SPEC.md §7). Every field that can be honestly absent is `null`, never
// a placeholder number — UNRATED is null, not zero.

export interface Pillars {
  reliability: number;
  track_record: number;
  safety: number;
  maturity: number;
}

export interface ScoreSource {
  provider: string;
  eval_id: string;
  settled_at: string;
  evidence_basis?: string;
  peer_assessment_count?: number;
}

export interface ReasonEvidence {
  source: string;
  field: string;
  quote?: string;
  locator?: string;
}

export interface Reason {
  code: string;
  severity: string;
  message: string;
  evidence?: ReasonEvidence;
  effect: string;
}

export interface LatencyMs {
  state: number;
  tier1: number;
  tier2?: number;
  total: number;
}

export interface PreSpendCheckResponse {
  receipt_id: string;
  decision: "APPROVE" | "HOLD" | "BLOCK";
  fico_score: number | null;
  fico_raw: number | null;
  decay: number | null;
  score_age_days: number | null;
  tier: string;
  max_safe_spend: number;
  pillars: Pillars | null;
  score_source: ScoreSource | null;
  tier_used: string[];
  latency_ms: LatencyMs;
  reasons: Reason[];
  ruleset_version: number;
  evidence_sha256: string;
  receipt_url: string;
}

export interface DirectoryEntry {
  counterparty_id: string;
  counterparty_name: string;
  target_url: string;
}

// SPEC §7.4: metrics are computed from real receipts, never constants. A
// metric without data is `null` and the desk renders "no data".
export interface DirectoryMetrics {
  receipts_total: number;
  decisions: { approve: number; hold: number; block: number };
  tiers: Record<string, number>;
  latency_ms: {
    tier1_only_p50: number | null;
    tier1_only_p95: number | null;
    escalated_p50: number | null;
    escalated_p95: number | null;
  };
  total_blocked_usd: number;
  cost_per_check_usd: null;
}

export interface DirectoryResponse {
  counterparties: DirectoryEntry[];
  metrics: DirectoryMetrics;
}

export interface PreSpendCheckRequest {
  counterparty_name: string;
  target_url: string;
  amount_usd: number;
  service_description: string;
}
