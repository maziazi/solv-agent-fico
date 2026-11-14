//! The decision-reason taxonomy (docs/SPEC.md §6).
//!
//! The taxonomy is closed: every reason carries a registered code, and
//! adding a code means editing SPEC §6.2 in the same PR (WORKFLOW §3.4).
//! The five conditions of SPEC §2.2 are enforced by [`Reason::satisfies_g5`].

use serde::{Deserialize, Serialize};

/// Longest allowed verbatim quote of untrusted counterparty text
/// (SPEC §6.1), truncated on a UTF-8 character boundary.
pub const MAX_QUOTE_CHARS: usize = 240;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Critical,
    High,
    Medium,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Effect {
    ForcedBlock,
    ForcedHold,
    LoweredCap,
    Informational,
}

/// Which engine produced the evidence (SPEC §6.1 `source`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum Source {
    ClefFlash,
    ServPromptGuard,
    ServReasoning,
    Reppo,
    PolicyGate,
}

/// Where a reason came from: which field, which engine, and a verbatim
/// quote when the reason derives from counterparty text (SPEC §2.2.3).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Evidence {
    pub source: Source,
    pub field: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub quote: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub locator: Option<String>,
    /// Raw detector output (probabilities, refusal flags, rule ids). The
    /// untrusted quote never appears here — only structured results.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub detector_output: Option<serde_json::Value>,
}

/// The closed reason-code taxonomy (SPEC §6.2). `Rule` carries the
/// registered `SOLV-RULE-` prefix with a per-rule id.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum ReasonCode {
    Injection001,
    Injection002,
    Injection003,
    Score001,
    Score002,
    Score003,
    Score010,
    Coldstart001,
    Coldstart002,
    Limit001,
    Limit002,
    Liveness001,
    Reason001,
    Reason002,
    System001,
    System002,
    System003,
    Rule(String),
}

impl ReasonCode {
    pub fn as_str(&self) -> String {
        match self {
            ReasonCode::Injection001 => "SOLV-INJECTION-001".to_string(),
            ReasonCode::Injection002 => "SOLV-INJECTION-002".to_string(),
            ReasonCode::Injection003 => "SOLV-INJECTION-003".to_string(),
            ReasonCode::Score001 => "SOLV-SCORE-001".to_string(),
            ReasonCode::Score002 => "SOLV-SCORE-002".to_string(),
            ReasonCode::Score003 => "SOLV-SCORE-003".to_string(),
            ReasonCode::Score010 => "SOLV-SCORE-010".to_string(),
            ReasonCode::Coldstart001 => "SOLV-COLDSTART-001".to_string(),
            ReasonCode::Coldstart002 => "SOLV-COLDSTART-002".to_string(),
            ReasonCode::Limit001 => "SOLV-LIMIT-001".to_string(),
            ReasonCode::Limit002 => "SOLV-LIMIT-002".to_string(),
            ReasonCode::Liveness001 => "SOLV-LIVENESS-001".to_string(),
            ReasonCode::Reason001 => "SOLV-REASON-001".to_string(),
            ReasonCode::Reason002 => "SOLV-REASON-002".to_string(),
            ReasonCode::System001 => "SOLV-SYSTEM-001".to_string(),
            ReasonCode::System002 => "SOLV-SYSTEM-002".to_string(),
            ReasonCode::System003 => "SOLV-SYSTEM-003".to_string(),
            ReasonCode::Rule(id) => format!("SOLV-RULE-{id}"),
        }
    }

    /// Default severity from the taxonomy's use of each code.
    pub fn default_severity(&self) -> Severity {
        match self {
            ReasonCode::Injection001
            | ReasonCode::Injection002
            | ReasonCode::Injection003
            | ReasonCode::Score001
            | ReasonCode::Score002
            | ReasonCode::Limit002 => Severity::Critical,
            ReasonCode::Score003 | ReasonCode::Liveness001 => Severity::Medium,
            ReasonCode::Score010 | ReasonCode::Coldstart002 | ReasonCode::System001 => {
                Severity::Info
            }
            _ => Severity::High,
        }
    }

    /// Default effect from SPEC §6.2. Rule codes defer to the rule's own
    /// effect; the taxonomy default for a rule is `forced_hold` when the
    /// caller does not state one.
    pub fn default_effect(&self) -> Effect {
        match self {
            ReasonCode::Injection001
            | ReasonCode::Injection002
            | ReasonCode::Injection003
            | ReasonCode::Score001
            | ReasonCode::Score002
            | ReasonCode::Limit002 => Effect::ForcedBlock,
            ReasonCode::Score003 | ReasonCode::Liveness001 => Effect::LoweredCap,
            ReasonCode::Score010 | ReasonCode::Coldstart002 | ReasonCode::System001 => {
                Effect::Informational
            }
            _ => Effect::ForcedHold,
        }
    }

    /// One human-readable sentence, no model jargon, no internal function
    /// names (SPEC §2.2.2).
    pub fn default_message(&self) -> String {
        match self {
            ReasonCode::Injection001 => "The screening layer refused the service description because it contains an instruction aimed at the paying agent.".to_string(),
            ReasonCode::Injection002 => "The service description contains an instruction addressed to the paying agent, not to a human reader.".to_string(),
            ReasonCode::Injection003 => "The request text tries to extract credentials or pressure the agent into acting immediately.".to_string(),
            ReasonCode::Score001 => "The counterparty's effective score is below the block floor.".to_string(),
            ReasonCode::Score002 => "The counterparty's safety assessment is critically low.".to_string(),
            ReasonCode::Score003 => "The score was reduced because the underlying evidence is aging.".to_string(),
            ReasonCode::Score010 => "Score and pillars meet the approval conditions.".to_string(),
            ReasonCode::Coldstart001 => "No settled third-party evaluation exists for this counterparty yet.".to_string(),
            ReasonCode::Coldstart002 => "An audit of this counterparty is queued and has not settled yet.".to_string(),
            ReasonCode::Limit001 => "The requested amount exceeds the spending cap for this counterparty's tier.".to_string(),
            ReasonCode::Limit002 => "The requested amount exceeds the operator's absolute cap.".to_string(),
            ReasonCode::Liveness001 => "The counterparty's service was unreachable at the last audit.".to_string(),
            ReasonCode::Reason001 => "The reasoning layer asked for the request to be revised before payment.".to_string(),
            ReasonCode::Reason002 => "The reasoning layer found a mismatch between the claim and the evidence.".to_string(),
            ReasonCode::System001 => "The fast screening layer timed out.".to_string(),
            ReasonCode::System002 => "The decision exceeded the latency budget.".to_string(),
            ReasonCode::System003 => "No decision layer was reachable, so payment is held.".to_string(),
            ReasonCode::Rule(_) => "A learned guardrail rule fired.".to_string(),
        }
    }
}

/// A single decision reason (SPEC §6.1).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Reason {
    pub code: ReasonCode,
    pub severity: Severity,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub evidence: Option<Evidence>,
    pub effect: Effect,
}

impl Reason {
    pub fn from_code(code: ReasonCode) -> Self {
        Self {
            severity: code.default_severity(),
            message: code.default_message(),
            effect: code.default_effect(),
            evidence: None,
            code,
        }
    }

    /// The SPEC §2.2 five-condition check used by the G5 test: registered
    /// code, human-readable message, and — for forcing effects — evidence.
    pub fn satisfies_g5(&self) -> bool {
        let registered = matches!(
            self.code.as_str().as_str(),
            "SOLV-INJECTION-001"
                | "SOLV-INJECTION-002"
                | "SOLV-INJECTION-003"
                | "SOLV-SCORE-001"
                | "SOLV-SCORE-002"
                | "SOLV-SCORE-003"
                | "SOLV-SCORE-010"
                | "SOLV-COLDSTART-001"
                | "SOLV-COLDSTART-002"
                | "SOLV-LIMIT-001"
                | "SOLV-LIMIT-002"
                | "SOLV-LIVENESS-001"
                | "SOLV-REASON-001"
                | "SOLV-REASON-002"
                | "SOLV-SYSTEM-001"
                | "SOLV-SYSTEM-002"
                | "SOLV-SYSTEM-003"
        ) || self.code.as_str().starts_with("SOLV-RULE-");
        if !registered {
            return false;
        }
        if self.message.trim().is_empty() {
            return false;
        }
        if matches!(self.effect, Effect::ForcedBlock | Effect::ForcedHold)
            && self.evidence.is_none()
        {
            return false;
        }
        true
    }
}

/// Truncates a verbatim quote of untrusted text to [`MAX_QUOTE_CHARS`]
/// characters, always on a UTF-8 character boundary (SPEC §6.1).
pub fn truncate_quote(quote: &str) -> String {
    if quote.chars().count() <= MAX_QUOTE_CHARS {
        return quote.to_string();
    }
    quote.chars().take(MAX_QUOTE_CHARS).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quote_truncates_on_char_boundaries() {
        let ascii = "a".repeat(300);
        assert_eq!(truncate_quote(&ascii).chars().count(), MAX_QUOTE_CHARS);

        // Each 'é' is two UTF-8 bytes; truncation must not split one.
        let multibyte = "é".repeat(300);
        let truncated = truncate_quote(&multibyte);
        assert_eq!(truncated.chars().count(), MAX_QUOTE_CHARS);
        assert!(std::str::from_utf8(truncated.as_bytes()).is_ok());
    }

    #[test]
    fn short_quotes_pass_through_verbatim() {
        assert_eq!(truncate_quote("short"), "short");
        let exact: String = "a".repeat(MAX_QUOTE_CHARS);
        assert_eq!(truncate_quote(&exact).chars().count(), MAX_QUOTE_CHARS);
    }

    #[test]
    fn rule_codes_carry_their_id() {
        assert_eq!(
            ReasonCode::Rule("014".to_string()).as_str(),
            "SOLV-RULE-014"
        );
    }
}
