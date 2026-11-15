//! `GET /receipt/:id` — the public trust receipt (docs/DESIGN.md §4.2,
//! docs/SPEC.md §7.3): a server-rendered, script-free audit record.
//!
//! Every value from the stored receipt is escaped before it enters markup,
//! and the only values used to pick a CSS class or a label come from fixed
//! vocabularies matched in code — never from the receipt text itself.

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::Response;
use serde_json::Value;

use super::escape::{escape, truncate_chars};
use super::{CSP_NO_SCRIPT, html_response, layout};
use crate::AppState;
use crate::api::iso8601_utc;
use crate::state::Receipt;

/// SPEC §6.1: a quote is shown to at most this many characters.
const QUOTE_MAX_CHARS: usize = 240;
/// Bound for every other counterparty- or model-supplied string.
const FIELD_MAX_CHARS: usize = 300;
const KEY_MAX_CHARS: usize = 40;
const EXTRA_EVIDENCE_KEYS: usize = 8;

pub async fn receipt_page(State(state): State<AppState>, Path(id): Path<String>) -> Response {
    match state.store().receipt(&id) {
        Some(receipt) => {
            let title = format!("Receipt {id} — Solv");
            html_response(
                StatusCode::OK,
                CSP_NO_SCRIPT,
                "public, max-age=300",
                layout::page(&title, &render_receipt(&id, &receipt)),
            )
        }
        None => html_response(
            StatusCode::NOT_FOUND,
            CSP_NO_SCRIPT,
            "no-store",
            layout::page("Receipt not found — Solv", &render_not_found()),
        ),
    }
}

/// The 404 body. It deliberately does not repeat the requested id.
pub fn render_not_found() -> String {
    "<section class=\"stamp stamp--unknown\"><h1>RECEIPT NOT FOUND</h1></section>\n\
     <p>No receipt with this identifier exists on this server.</p>"
        .to_string()
}

/// Renders the `<main>` content for a stored receipt. Never panics: every
/// field is read defensively, and a receipt stored before the receipt store
/// existed (null `response`/`evidence`) renders with an explicit notice.
pub fn render_receipt(id: &str, receipt: &Receipt) -> String {
    let response = &receipt.response;
    let evidence = &receipt.evidence;
    let mut out = String::new();

    out.push_str(&stamp_section(id, receipt));

    if !response.is_object() {
        out.push_str(
            "<section class=\"notice\"><h2>Partial record</h2>\n\
             <p>This receipt was written before full receipts existed. Only the decision, \
             amount, time, and hash below were kept; the score, reasons, and timings were not.</p>\
             </section>\n",
        );
        out.push_str(&hash_section(id, receipt, None));
        return out;
    }

    out.push_str(&score_section(response));
    out.push_str(&provenance_section(response));
    out.push_str(&pillars_section(response));
    out.push_str(&reasons_section(response));
    out.push_str(&pipeline_section(response));
    let ruleset = evidence
        .get("ruleset_version")
        .and_then(Value::as_u64)
        .or_else(|| response.get("ruleset_version").and_then(Value::as_u64));
    out.push_str(&hash_section(id, receipt, ruleset));
    out
}

// ---------------------------------------------------------------------------
// Sections
// ---------------------------------------------------------------------------

fn stamp_section(id: &str, receipt: &Receipt) -> String {
    let money = format_usd(receipt.amount_usd);
    let (class, text) = match receipt.decision.as_str() {
        "APPROVE" => ("approve", format!("APPROVED — {money}")),
        "BLOCK" => ("block", format!("BLOCKED — {money} NOT SPENT")),
        "HOLD" => ("hold", format!("HELD — {money} NOT SPENT")),
        _ => ("unknown", "DECISION NOT RECOGNISED".to_string()),
    };
    let request = receipt.evidence.get("request");
    let counterparty = request
        .and_then(|r| r.get("counterparty_id"))
        .and_then(Value::as_str);
    let target = request
        .and_then(|r| r.get("target_url"))
        .and_then(Value::as_str);

    let mut out = format!(
        "<section class=\"stamp stamp--{class}\" aria-labelledby=\"stamp-title\">\n\
         <h1 id=\"stamp-title\">{}</h1>\n</section>\n",
        escape(&text)
    );
    out.push_str(
        "<section class=\"ledger\" aria-label=\"Receipt details\">\n<dl class=\"facts\">\n",
    );
    out.push_str(&fact("Receipt", &mono(id)));
    out.push_str(&fact(
        "Issued",
        &mono(&iso8601_utc(receipt.created_at_unix)),
    ));
    out.push_str(&fact(
        "Counterparty",
        &untrusted_inline(counterparty, FIELD_MAX_CHARS),
    ));
    out.push_str(&fact("Target", &untrusted_inline(target, FIELD_MAX_CHARS)));
    out.push_str("</dl>\n<p class=\"micro\">Counterparty and target are supplied by the party being paid and are shown as plain text only.</p>\n</section>\n");
    out
}

fn score_section(response: &Value) -> String {
    let tier = response.get("tier").and_then(Value::as_str).unwrap_or("");
    let tier_class = match tier {
        "PRIME" => "approve",
        "STANDARD" | "SUBPRIME" | "UNRATED" => "hold",
        "HIGH_RISK" => "block",
        _ => "unknown",
    };
    let tier_text = if tier_class == "unknown" {
        "TIER UNKNOWN"
    } else {
        tier
    };

    let effective = response.get("fico_score").and_then(Value::as_u64);
    let mut out = String::from("<section class=\"card\" aria-labelledby=\"score-title\">\n");
    out.push_str("<h2 id=\"score-title\">Score</h2>\n");
    out.push_str(&format!(
        "<p><span class=\"badge badge--{tier_class}\">{}</span></p>\n",
        escape(tier_text)
    ));
    match effective {
        Some(effective) => {
            let raw = response.get("fico_raw").and_then(Value::as_u64);
            let decay = response.get("decay").and_then(Value::as_u64);
            out.push_str(&format!(
                "<p class=\"display mono\">{effective}<span class=\"unit\"> / 850</span></p>\n"
            ));
            out.push_str(&gauge(effective, raw, decay));
            let mut note = String::from("Effective score.");
            if let Some(raw) = raw {
                note.push_str(&format!(" Raw {raw}"));
                if let Some(decay) = decay {
                    note.push_str(&format!(", decay {decay}"));
                }
                note.push('.');
            }
            out.push_str(&format!("<p class=\"small\">{}</p>\n", escape(&note)));
        }
        None => {
            out.push_str(
                "<p class=\"display\">UNRATED</p>\n\
                 <p class=\"small\">No settled evaluation backs this decision, so there is no score to show.</p>\n",
            );
        }
    }
    out.push_str("</section>\n");
    out
}

fn provenance_section(response: &Value) -> String {
    let mut out = String::from("<section class=\"card\" aria-labelledby=\"prov-title\">\n");
    out.push_str("<h2 id=\"prov-title\">Score provenance</h2>\n");
    match response.get("score_source").filter(|v| v.is_object()) {
        Some(source) => {
            let provider = source.get("provider").and_then(Value::as_str);
            let eval_id = source.get("eval_id").and_then(Value::as_str);
            let settled = source.get("settled_at").and_then(Value::as_str);
            out.push_str("<dl class=\"facts\">\n");
            out.push_str(&fact(
                "Provider",
                &untrusted_inline(provider, KEY_MAX_CHARS),
            ));
            out.push_str(&fact(
                "Evaluation",
                &untrusted_inline(eval_id, FIELD_MAX_CHARS),
            ));
            out.push_str(&fact("Settled", &untrusted_inline(settled, KEY_MAX_CHARS)));
            out.push_str("</dl>\n");
            if let Some(age) = response.get("score_age_days").and_then(Value::as_u64) {
                out.push_str(&format!(
                    "<p class=\"display mono\">{age}<span class=\"unit\"> days old</span></p>\n"
                ));
            }
        }
        None => out.push_str("<p>No settled evaluation: nothing to attribute.</p>\n"),
    }
    out.push_str("</section>\n");
    out
}

fn pillars_section(response: &Value) -> String {
    let Some(pillars) = response.get("pillars").filter(|v| v.is_object()) else {
        return String::new();
    };
    let mut out = String::from("<section class=\"card\" aria-labelledby=\"pillars-title\">\n");
    out.push_str("<h2 id=\"pillars-title\">Four pillars</h2>\n<div class=\"scroll\">\n<table>\n");
    out.push_str("<thead><tr><th scope=\"col\">Pillar</th><th scope=\"col\">Score</th></tr></thead>\n<tbody>\n");
    for (label, key) in [
        ("Reliability", "reliability"),
        ("Track record", "track_record"),
        ("Safety", "safety"),
        ("Maturity", "maturity"),
    ] {
        let value = match pillars.get(key).and_then(Value::as_u64) {
            Some(value) => format!("{value} / 10"),
            None => "—".to_string(),
        };
        out.push_str(&format!(
            "<tr><th scope=\"row\">{label}</th><td class=\"mono\">{}</td></tr>\n",
            escape(&value)
        ));
    }
    out.push_str("</tbody>\n</table>\n</div>\n</section>\n");
    out
}

fn reasons_section(response: &Value) -> String {
    let mut out = String::from("<section class=\"card\" aria-labelledby=\"reasons-title\">\n");
    out.push_str("<h2 id=\"reasons-title\">Reasons</h2>\n");
    let reasons = response.get("reasons").and_then(Value::as_array);
    match reasons.filter(|reasons| !reasons.is_empty()) {
        Some(reasons) => {
            out.push_str("<ol class=\"reasons\">\n");
            for reason in reasons {
                out.push_str(&reason_row(reason));
            }
            out.push_str("</ol>\n");
        }
        None => out.push_str("<p>No reasons were recorded on this receipt.</p>\n"),
    }
    out.push_str("</section>\n");
    out
}

fn reason_row(reason: &Value) -> String {
    let code = reason.get("code").and_then(Value::as_str);
    let severity = reason.get("severity").and_then(Value::as_str).unwrap_or("");
    let severity_class = match severity {
        "critical" => "block",
        "high" | "medium" => "hold",
        "info" => "info",
        _ => "unknown",
    };
    let effect = reason.get("effect").and_then(Value::as_str);
    let message = reason.get("message").and_then(Value::as_str);

    let mut out = String::from("<li class=\"reason\">\n<p class=\"reason-head\">");
    out.push_str(&format!(
        "<code class=\"reason-code\">{}</code> <span class=\"chip chip--{severity_class}\">{}</span> <span class=\"chip\">{}</span>",
        escape(truncate_chars(code.unwrap_or("UNKNOWN-CODE"), KEY_MAX_CHARS).0),
        escape(truncate_chars(severity, 16).0),
        escape(truncate_chars(effect.unwrap_or("effect unknown"), KEY_MAX_CHARS).0),
    ));
    out.push_str("</p>\n");
    out.push_str(&format!(
        "<p>{}</p>\n",
        escape(truncate_chars(message.unwrap_or("No message recorded."), FIELD_MAX_CHARS).0)
    ));
    if let Some(evidence) = reason.get("evidence").filter(|v| v.is_object()) {
        out.push_str(&evidence_block(evidence));
    }
    out.push_str("</li>\n");
    out
}

fn evidence_block(evidence: &Value) -> String {
    let Some(map) = evidence.as_object() else {
        return String::new();
    };
    let mut out = String::from("<div class=\"evidence\">\n<dl class=\"facts facts--tight\">\n");
    for key in ["source", "field", "locator"] {
        if let Some(text) = map.get(key).and_then(Value::as_str) {
            out.push_str(&fact(
                key,
                &format!(
                    "<code>{}</code>",
                    escape(truncate_chars(text, FIELD_MAX_CHARS).0)
                ),
            ));
        }
    }
    let extras = map
        .iter()
        .filter(|(key, _)| !matches!(key.as_str(), "source" | "field" | "locator" | "quote"))
        .take(EXTRA_EVIDENCE_KEYS);
    for (key, value) in extras {
        let compact = value.to_string();
        out.push_str(&fact(
            truncate_chars(key, KEY_MAX_CHARS).0,
            &format!(
                "<code>{}</code>",
                escape(truncate_chars(&compact, FIELD_MAX_CHARS).0)
            ),
        ));
    }
    out.push_str("</dl>\n");
    if let Some(quote) = map.get("quote").and_then(Value::as_str) {
        let (shown, cut) = truncate_chars(quote, QUOTE_MAX_CHARS);
        out.push_str(&format!(
            "<figure class=\"quote\">\n<figcaption>untrusted counterparty text</figcaption>\n<pre>{}</pre>\n",
            escape(shown)
        ));
        if cut {
            out.push_str(&format!(
                "<p class=\"micro\">Shown truncated to {QUOTE_MAX_CHARS} characters.</p>\n"
            ));
        }
        out.push_str("</figure>\n");
    }
    out.push_str("</div>\n");
    out
}

fn pipeline_section(response: &Value) -> String {
    let mut out = String::from("<section class=\"card\" aria-labelledby=\"pipe-title\">\n");
    out.push_str("<h2 id=\"pipe-title\">Decision pipeline</h2>\n");
    let used: Vec<&str> = response
        .get("tier_used")
        .and_then(Value::as_array)
        .map(|tiers| tiers.iter().filter_map(Value::as_str).collect())
        .unwrap_or_default();
    let used_text = if used.is_empty() {
        "not recorded".to_string()
    } else {
        used.join(", ")
    };
    out.push_str(&format!(
        "<p class=\"small\">Tiers that ran: <code>{}</code></p>\n",
        escape(truncate_chars(&used_text, FIELD_MAX_CHARS).0)
    ));
    if let Some(latency) = response.get("latency_ms").filter(|v| v.is_object()) {
        out.push_str("<div class=\"scroll\">\n<table>\n<thead><tr><th scope=\"col\">Step</th><th scope=\"col\">Elapsed</th></tr></thead>\n<tbody>\n");
        for (label, key) in [
            ("State lookup", "state"),
            ("Clef-flash (Tier 1)", "tier1"),
            ("SERV Reasoning (Tier 2)", "tier2"),
            ("Total", "total"),
        ] {
            let cell = match latency.get(key).and_then(Value::as_u64) {
                Some(ms) => format!("{ms} ms"),
                None if key == "tier2" => "did not run".to_string(),
                None => "—".to_string(),
            };
            out.push_str(&format!(
                "<tr><th scope=\"row\">{label}</th><td class=\"mono\">{}</td></tr>\n",
                escape(&cell)
            ));
        }
        out.push_str("</tbody>\n</table>\n</div>\n");
    }
    out.push_str("</section>\n");
    out
}

fn hash_section(id: &str, receipt: &Receipt, ruleset: Option<u64>) -> String {
    let mut out = String::from("<section class=\"card\" aria-labelledby=\"hash-title\">\n");
    out.push_str("<h2 id=\"hash-title\">Evidence hash</h2>\n");
    if receipt.evidence_sha256.is_empty() {
        out.push_str("<p>No evidence hash was recorded on this receipt.</p>\n");
    } else {
        out.push_str(&format!(
            "<p class=\"hash mono\">{}</p>\n",
            escape(truncate_chars(&receipt.evidence_sha256, 128).0)
        ));
    }
    if let Some(ruleset) = ruleset {
        out.push_str(&format!(
            "<p class=\"small\">Ruleset version <span class=\"mono\">{ruleset}</span></p>\n"
        ));
    }
    if receipt.evidence.is_object() {
        out.push_str(&format!(
            "<p class=\"small\">Anyone can recompute this hash. The hash is SHA-256 of the receipt's <code>evidence</code> in canonical JSON: keys sorted by UTF-8 bytes, no whitespace, floats written as Python's <code>repr</code> (docs/SPEC.md §6.3). The reference verifier is <code>tools/recompute_receipt.py</code>, which uses only the Python standard library.</p>\n\
             <pre class=\"cmd\">curl -s \"$SOLV_HOST/api/v1/receipt/{}\" &gt; receipt.json\npython3 tools/recompute_receipt.py receipt.json</pre>\n",
            escape(id)
        ));
    }
    out.push_str("</section>\n");
    out
}

// ---------------------------------------------------------------------------
// Pieces
// ---------------------------------------------------------------------------

/// FICO track 300→850 with the tier floors (550, 640, 720) ticked and the
/// raw score hairlined when decay is non-zero (docs/DESIGN.md §5). Built
/// from SVG attributes only — no inline `style`, which the CSP forbids. The
/// score is also printed beside it, so the gauge is never the only reading.
fn gauge(effective: u64, raw: Option<u64>, decay: Option<u64>) -> String {
    let position = |score: u64| score.clamp(300, 850) - 300;
    let mut out = format!(
        "<svg class=\"gauge\" viewBox=\"0 0 550 48\" role=\"img\" aria-label=\"Score {} on a scale from 300 to 850\" focusable=\"false\">\n\
         <rect class=\"gauge-track\" x=\"0\" y=\"12\" width=\"550\" height=\"16\"/>\n\
         <rect class=\"gauge-fill\" x=\"0\" y=\"12\" width=\"{}\" height=\"16\"/>\n",
        effective.clamp(300, 850),
        position(effective)
    );
    for floor in [550u64, 640, 720] {
        let x = position(floor);
        out.push_str(&format!(
            "<line class=\"gauge-tick\" x1=\"{x}\" y1=\"6\" x2=\"{x}\" y2=\"34\"/>\n\
             <text class=\"gauge-label\" x=\"{x}\" y=\"46\" text-anchor=\"middle\">{floor}</text>\n"
        ));
    }
    if let (Some(raw), Some(decay)) = (raw, decay) {
        if decay > 0 {
            let x = position(raw);
            out.push_str(&format!(
                "<line class=\"gauge-raw\" x1=\"{x}\" y1=\"4\" x2=\"{x}\" y2=\"36\"/>\n"
            ));
        }
    }
    out.push_str(
        "<text class=\"gauge-label\" x=\"0\" y=\"46\" text-anchor=\"start\">300</text>\n\
         <text class=\"gauge-label\" x=\"550\" y=\"46\" text-anchor=\"end\">850</text>\n</svg>\n",
    );
    out
}

fn fact(label: &str, value_html: &str) -> String {
    format!(
        "<div><dt>{}</dt><dd>{value_html}</dd></div>\n",
        escape(label)
    )
}

fn mono(text: &str) -> String {
    format!(
        "<code>{}</code>",
        escape(truncate_chars(text, FIELD_MAX_CHARS).0)
    )
}

/// A counterparty- or provider-supplied string, bounded and escaped.
fn untrusted_inline(text: Option<&str>, max_chars: usize) -> String {
    match text {
        Some(text) => {
            let (shown, cut) = truncate_chars(text, max_chars);
            let ellipsis = if cut { "…" } else { "" };
            format!("<code>{}{ellipsis}</code>", escape(shown))
        }
        None => "<span class=\"muted\">not recorded</span>".to_string(),
    }
}

/// `$50.00`; micro amounts keep their significant digits instead of
/// rounding to `$0.00`.
fn format_usd(amount: f64) -> String {
    if !amount.is_finite() {
        "an unreadable amount".to_string()
    } else if amount == 0.0 || (0.01..1e15).contains(&amount.abs()) {
        format!("${amount:.2}")
    } else if amount.abs() < 0.01 {
        let text = format!("{amount:.9}");
        format!("${}", text.trim_end_matches('0'))
    } else {
        format!("${amount:e}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn usd_formatting_keeps_micro_amounts_visible() {
        assert_eq!(format_usd(50.0), "$50.00");
        assert_eq!(format_usd(0.0), "$0.00");
        assert_eq!(format_usd(0.000032), "$0.000032");
        assert_eq!(format_usd(1e-9), "$0.000000001");
        assert_eq!(format_usd(1e300), "$1e300");
        assert_eq!(format_usd(f64::NAN), "an unreadable amount");
    }

    #[test]
    fn gauge_clamps_out_of_range_scores() {
        let low = gauge(0, None, None);
        assert!(low.contains("width=\"0\""));
        let high = gauge(u64::MAX, Some(u64::MAX), Some(3));
        assert!(high.contains("width=\"550\""));
        assert!(!high.contains("style="));
    }
}
