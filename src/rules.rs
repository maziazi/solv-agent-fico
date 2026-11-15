//! Learned guardrails (docs/SPEC.md §5.5): a settled Reppo critique becomes
//! a `shadow` rule, and an `active` rule compiles into one more `noul`
//! question on the next Clef-flash call.
//!
//! The critique is untrusted text, so it never becomes question text.
//! Distillation only *selects* one of a fixed catalogue of topics by
//! keyword; the question Clef reads is the catalogue's own wording. The
//! critique survives only as a bounded, sanitised quote on the rule so an
//! operator can contest it, and is never sent to a model.

use std::sync::Arc;

use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

use crate::policy::{FiredRule, RuleEffect};
use crate::reason::{Reason, ReasonCode};
use crate::receipt::RULESET_VERSION;
use crate::reppo::{
    CriterionVerdict, MAX_UNTRUSTED_CHARS, is_invisible_format_char, sanitize_untrusted,
};
use crate::state::{LearnedRule, RuleStatus, Store};
use crate::tier1::{ClefQuestion, ClefResponse, QuestionKind};

/// Active rules injected into one Clef call (SPEC §5.5).
pub const MAX_ACTIVE_RULES_PER_CALL: usize = 8;
/// Rules the store keeps, in any status. Bounds memory and the snapshot.
pub const MAX_STORED_RULES: usize = 256;
/// A critique enters distillation at this verdict score or below.
pub const DISTILL_MAX_VERDICT_SCORE: f64 = 4.0;
/// Probability of `true` at which a rule fires, unless the rule says
/// otherwise.
pub const DEFAULT_THRESHOLD: f64 = 0.85;
/// Longest critique quote kept on a rule, in characters.
pub const MAX_QUOTE_CHARS: usize = 300;
/// Rule ids are `SOLV-RULE-` plus a number; the reason code is the tail.
pub const RULE_ID_PREFIX: &str = "SOLV-RULE-";
/// Longest question key built from a rule id.
const MAX_KEY_CHARS: usize = 32;
/// How much of a critique topic selection reads, in characters.
const MAX_SELECTION_CHARS: usize = 2000;
/// A keyword is ignored when a negator sits within this many words
/// before it ("no refund", "never a refund"). Longer-range negation
/// ("never offers a refund") is not detected.
pub const NEGATION_WINDOW: usize = 2;

/// Reads a stored topic, mapping anything this build does not know (a
/// topic added by a newer build, a hand-edited value, the wrong type) to
/// `None` instead of failing the whole snapshot. A rule without a topic is
/// kept but never compiled, exactly like a rule that predates topics.
pub fn lenient_topic<'de, D>(deserializer: D) -> Result<Option<RuleTopic>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = Option::<Value>::deserialize(deserializer)?;
    Ok(value.and_then(|value| serde_json::from_value(value).ok()))
}

/// What a learned rule asks about. The set is closed: a critique that
/// matches none of these produces no rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RuleTopic {
    GuaranteedRefund,
    UrgencyPressure,
    CredentialRequest,
    OffPlatformPayment,
    UnverifiableClaim,
    ImpersonatedAuthority,
}

/// The fixed wording and matching keywords of one topic.
pub struct TopicSpec {
    /// Lowercase fragments searched in the critique.
    pub keywords: &'static [&'static str],
    pub instructions: &'static str,
    pub true_text: &'static str,
    pub false_text: &'static str,
}

/// The catalogue topics whose readings the standard `risk_class` question
/// already covers (SPEC §1.3: urgency pressure → coercion, credential
/// request → exfiltration, unverifiable claim → overclaim). Documented set,
/// pinned by `three_of_six_topics_overlap_the_risk_class_question`.
pub const TOPICS_OVERLAPPING_RISK_CLASS: [RuleTopic; 3] = [
    RuleTopic::UrgencyPressure,
    RuleTopic::CredentialRequest,
    RuleTopic::UnverifiableClaim,
];

impl RuleTopic {
    /// Catalogue order; also the tie-break between equally good matches.
    pub const ALL: [RuleTopic; 6] = [
        RuleTopic::GuaranteedRefund,
        RuleTopic::UrgencyPressure,
        RuleTopic::CredentialRequest,
        RuleTopic::OffPlatformPayment,
        RuleTopic::UnverifiableClaim,
        RuleTopic::ImpersonatedAuthority,
    ];

    pub fn spec(self) -> &'static TopicSpec {
        match self {
            RuleTopic::GuaranteedRefund => &TopicSpec {
                keywords: &["refund", "money back", "insur", "reimburse", "compensat"],
                instructions: "Does the description promise a guaranteed refund or insurance if the deliverable fails?",
                true_text: "States or implies a guaranteed refund or insurance.",
                false_text: "Makes no such promise.",
            },
            RuleTopic::UrgencyPressure => &TopicSpec {
                keywords: &[
                    "urgent",
                    "immediately",
                    "pressure",
                    "deadline",
                    "act now",
                    "limited time",
                    "time-limited",
                    "rush",
                ],
                instructions: "Does the description pressure the paying agent to pay immediately or before a short deadline?",
                true_text: "Pushes the agent to act at once or within a short window.",
                false_text: "Sets no pressure on when to pay.",
            },
            RuleTopic::CredentialRequest => &TopicSpec {
                keywords: &[
                    "credential",
                    "password",
                    "private key",
                    "seed phrase",
                    "api key",
                    "wallet access",
                    "login",
                ],
                instructions: "Does the description ask for credentials, keys, seed phrases, or wallet access?",
                true_text: "Asks the agent or its owner for secrets or access.",
                false_text: "Asks for no secrets or access.",
            },
            RuleTopic::OffPlatformPayment => &TopicSpec {
                keywords: &[
                    "off platform",
                    "different address",
                    "another wallet",
                    "other wallet",
                    "external wallet",
                    "redirect",
                    "payment address",
                ],
                instructions: "Does the description direct payment to a channel or address other than the stated target_url?",
                true_text: "Names a different payment channel or address.",
                false_text: "Keeps payment on the stated target_url.",
            },
            RuleTopic::UnverifiableClaim => &TopicSpec {
                keywords: &[
                    "unverifiable",
                    "unverified",
                    "no evidence",
                    "unsubstantiated",
                    "guaranteed return",
                    "guaranteed profit",
                    "exaggerat",
                    "overclaim",
                ],
                instructions: "Does the description make performance claims that cannot be verified?",
                true_text: "Claims results with no way to verify them.",
                false_text: "Claims only what can be checked.",
            },
            RuleTopic::ImpersonatedAuthority => &TopicSpec {
                keywords: &[
                    "impersonat",
                    "affiliat",
                    "endorse",
                    "official",
                    "authority",
                    "spoof",
                ],
                instructions: "Does the description claim an affiliation or endorsement it does not evidence?",
                true_text: "Claims an affiliation or endorsement without evidence.",
                false_text: "Claims no unevidenced affiliation.",
            },
        }
    }

    /// Chooses the topic for `critique`: the topic with the most distinct
    /// keywords found as whole words (a keyword may carry a suffix, so
    /// "refunds" matches "refund", but never sits inside another word) and
    /// not negated. Two topics tied for most keywords is [`TopicChoice::Ambiguous`]:
    /// a guess here would put a guardrail on the wrong question.
    pub fn choose(critique: &str) -> TopicChoice {
        let words = words(&selection_text(critique));
        let mut scores: Vec<(RuleTopic, usize)> = Vec::new();
        for topic in RuleTopic::ALL {
            let hits = topic
                .spec()
                .keywords
                .iter()
                .filter(|keyword| keyword_found(&words, keyword))
                .count();
            if hits > 0 {
                scores.push((topic, hits));
            }
        }
        let Some(top) = scores.iter().map(|(_, hits)| *hits).max() else {
            return TopicChoice::NoMatch;
        };
        let mut leaders = scores.iter().filter(|(_, hits)| *hits == top);
        match (leaders.next(), leaders.next()) {
            (Some((topic, _)), None) => TopicChoice::Selected(*topic),
            _ => TopicChoice::Ambiguous,
        }
    }

    /// The selected topic, or `None` for no match and for a tie.
    pub fn from_critique(critique: &str) -> Option<RuleTopic> {
        match Self::choose(critique) {
            TopicChoice::Selected(topic) => Some(topic),
            TopicChoice::NoMatch | TopicChoice::Ambiguous => None,
        }
    }
}

/// How topic selection ended for one critique.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopicChoice {
    Selected(RuleTopic),
    /// Several topics tied for the most keywords.
    Ambiguous,
    NoMatch,
}

/// The critique as topic selection reads it: whitespace of any kind becomes
/// one space, and control and invisible characters are dropped, so text
/// cannot hide a keyword by splitting it ("re\u{200B}fund") or by routing
/// around it. Bounded, because the critique comes from outside.
fn selection_text(critique: &str) -> String {
    critique
        .chars()
        .take(MAX_SELECTION_CHARS)
        .filter_map(|c| {
            if c.is_whitespace() {
                Some(' ')
            } else if c.is_control() || is_invisible_format_char(c) {
                None
            } else {
                Some(c)
            }
        })
        .collect()
}

/// Lowercase alphanumeric words. Everything else separates words, except
/// an apostrophe inside a word, which stays ("doesn't").
fn words(text: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut current = String::new();
    for c in text.chars() {
        if c.is_alphanumeric() {
            current.extend(c.to_lowercase());
        } else if (c == '\'' || c == '\u{2019}') && !current.is_empty() {
            current.push('\'');
        } else if !current.is_empty() {
            out.push(std::mem::take(&mut current));
        }
    }
    if !current.is_empty() {
        out.push(current);
    }
    for word in &mut out {
        while word.ends_with('\'') {
            word.pop();
        }
    }
    out.retain(|word| !word.is_empty());
    out
}

fn is_negator(word: &str) -> bool {
    matches!(
        word,
        "no" | "not"
            | "never"
            | "without"
            | "cannot"
            | "non"
            | "none"
            | "nor"
            | "cant"
            | "dont"
            | "doesnt"
            | "didnt"
            | "wont"
            | "isnt"
            | "arent"
            | "wasnt"
    ) || word.ends_with("n't")
}

/// Whether `keyword` (one or more words, each matched as the start of a
/// word) occurs in `words` and is not negated: no negator within
/// [`NEGATION_WINDOW`] words before it, and not followed by "free"
/// ("login-free").
fn keyword_found(words: &[String], keyword: &str) -> bool {
    let wanted = self::words(keyword);
    if wanted.is_empty() || words.len() < wanted.len() {
        return false;
    }
    (0..=words.len() - wanted.len()).any(|start| {
        let matches = wanted
            .iter()
            .enumerate()
            .all(|(offset, part)| words[start + offset].starts_with(part.as_str()));
        if !matches {
            return false;
        }
        let end = start + wanted.len();
        let window_start = start.saturating_sub(NEGATION_WINDOW);
        let negated = words[window_start..start]
            .iter()
            .any(|word| is_negator(word))
            || words.get(end).is_some_and(|next| next == "free");
        !negated
    })
}

/// Where a rule came from, so it can be contested (SPEC §5.5).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuleSource {
    pub eval_id: String,
    pub criterion: String,
    /// Bounded and sanitised critique text. Stored for the operator to read;
    /// never put into a question or a prompt.
    pub critique_quote: String,
}

/// A rule before the store gives it an id.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleDraft {
    pub topic: RuleTopic,
    pub source: RuleSource,
}

/// What [`distil_detailed`] made of one evaluation's verdicts.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct Distilled {
    pub drafts: Vec<RuleDraft>,
    /// Low-scoring critiques that matched several topics equally.
    pub ambiguous: usize,
}

/// Turns the verdicts of one settled evaluation into rule drafts: one per
/// low-scoring critique that clearly matches one topic, at most one per
/// topic. A critique that matches nothing, or ties between topics, makes
/// no rule.
pub fn distil_detailed(eval_id: &str, verdicts: &[CriterionVerdict]) -> Distilled {
    let mut distilled = Distilled::default();
    for verdict in verdicts {
        if !verdict.score.is_finite() || verdict.score > DISTILL_MAX_VERDICT_SCORE {
            continue;
        }
        let topic = match RuleTopic::choose(&verdict.critique) {
            TopicChoice::Selected(topic) => topic,
            TopicChoice::Ambiguous => {
                distilled.ambiguous += 1;
                continue;
            }
            TopicChoice::NoMatch => continue,
        };
        if distilled.drafts.iter().any(|draft| draft.topic == topic) {
            continue;
        }
        distilled.drafts.push(RuleDraft {
            topic,
            source: RuleSource {
                eval_id: sanitize_untrusted(eval_id, MAX_UNTRUSTED_CHARS),
                criterion: sanitize_untrusted(&verdict.criterion, MAX_UNTRUSTED_CHARS),
                critique_quote: sanitize_untrusted(&verdict.critique, MAX_QUOTE_CHARS),
            },
        });
    }
    distilled
}

/// The drafts of [`distil_detailed`].
pub fn distil(eval_id: &str, verdicts: &[CriterionVerdict]) -> Vec<RuleDraft> {
    distil_detailed(eval_id, verdicts).drafts
}

/// What [`distil_into_store`] did, for the job log.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct DistilReport {
    /// Ids of the new `shadow` rules.
    pub created: Vec<String>,
    /// Drafts dropped because the topic already has a rule.
    pub duplicates: usize,
    /// Drafts dropped because the store is full.
    pub store_full: usize,
    /// Critiques that matched several topics equally and so made no rule.
    pub ambiguous: usize,
}

/// Distils `verdicts` and stores the results as `shadow` rules. Runs in the
/// Reppo worker, never on the hot path. A critique that matches no topic
/// leaves no trace beyond the evaluation itself; a tie is counted and
/// logged without the critique text.
pub fn distil_into_store(
    store: &Store,
    eval_id: &str,
    verdicts: &[CriterionVerdict],
) -> DistilReport {
    let distilled = distil_detailed(eval_id, verdicts);
    let mut report = DistilReport {
        ambiguous: distilled.ambiguous,
        ..DistilReport::default()
    };
    if distilled.ambiguous > 0 {
        eprintln!(
            "solv: evaluation {} had {} critique(s) matching several topics equally; no rule created for them",
            sanitize_untrusted(eval_id, MAX_UNTRUSTED_CHARS),
            distilled.ambiguous
        );
    }
    for draft in distilled.drafts {
        match store.add_learned_rule(draft) {
            AddRule::Added(rule_id) => report.created.push(rule_id),
            AddRule::Duplicate => report.duplicates += 1,
            AddRule::StoreFull => report.store_full += 1,
        }
    }
    report
}

/// The result of offering a draft to the store.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AddRule {
    Added(String),
    Duplicate,
    StoreFull,
}

/// One active rule, ready to ask and to check.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledRule {
    /// `SOLV-RULE-014`, as stored.
    pub rule_id: String,
    pub topic: RuleTopic,
    /// The question key in the Clef request and answers.
    pub key: String,
    pub effect: RuleEffect,
    pub threshold: f64,
}

impl CompiledRule {
    /// The reason-code tail (`014`): `SOLV-RULE-<id>`.
    fn reason_tail(&self) -> String {
        self.rule_id
            .strip_prefix(RULE_ID_PREFIX)
            .unwrap_or(&self.rule_id)
            .to_string()
    }
}

/// The active rule set compiled once, when rules change, and shared with
/// the hot path as an `Arc` so a request pays no recompilation.
#[derive(Debug, Clone, PartialEq)]
pub struct CompiledRules {
    /// The `ruleset_version` decisions under this set carry.
    pub ruleset_version: u32,
    pub rules: Vec<CompiledRule>,
    pub questions: Vec<ClefQuestion>,
}

/// What a Clef response says about the compiled rules.
#[derive(Debug, Default, Clone, PartialEq)]
pub struct RuleOutcome {
    /// Rules whose answer reached their threshold.
    pub fired: Vec<FiredRule>,
    /// Ids of the rules the response answered: the rules that were in
    /// force for the decision (SPEC §6.3 `rules_applied`).
    pub applied: Vec<String>,
    /// For each fired rule, its reason-code tail and the structured detail
    /// that goes on the reason. Never the critique.
    pub details: Vec<(String, Value)>,
}

impl CompiledRules {
    pub fn empty(epoch: u32) -> Arc<Self> {
        Arc::new(Self {
            ruleset_version: RULESET_VERSION.saturating_add(epoch),
            rules: Vec::new(),
            questions: Vec::new(),
        })
    }

    /// Whether `other` asks the same questions with the same effects: the
    /// epoch aside, nothing in force differs.
    pub fn same_set(&self, other: &CompiledRules) -> bool {
        self.rules == other.rules && self.questions == other.questions
    }

    /// Compiles the `active` rules of `rules`, at most
    /// [`MAX_ACTIVE_RULES_PER_CALL`], lowest id first. A rule without a
    /// topic (it predates PR-10), with a threshold outside `(0, 1]`, or
    /// with an id that yields no usable key is skipped, not guessed at.
    pub fn compile(rules: &[LearnedRule], epoch: u32) -> Arc<Self> {
        let mut ordered: Vec<&LearnedRule> = rules
            .iter()
            .filter(|rule| rule.status == RuleStatus::Active)
            .collect();
        ordered.sort_by_key(|rule| (rule_number(&rule.rule_id), rule.rule_id.clone()));

        let mut compiled = Vec::new();
        let mut questions = Vec::new();
        for rule in ordered {
            if compiled.len() >= MAX_ACTIVE_RULES_PER_CALL {
                break;
            }
            let Some(topic) = rule.topic else { continue };
            if !(rule.threshold > 0.0 && rule.threshold <= 1.0) {
                continue;
            }
            let Some(key) = question_key(&rule.rule_id) else {
                continue;
            };
            if compiled
                .iter()
                .any(|existing: &CompiledRule| existing.key == key)
            {
                continue;
            }
            let spec = topic.spec();
            questions.push(ClefQuestion {
                key: key.clone(),
                instructions: spec.instructions.to_string(),
                kind: QuestionKind::Noul {
                    true_text: Some(spec.true_text.to_string()),
                    false_text: Some(spec.false_text.to_string()),
                },
            });
            compiled.push(CompiledRule {
                rule_id: rule.rule_id.clone(),
                topic,
                key,
                effect: rule.effect,
                threshold: rule.threshold,
            });
        }
        Arc::new(Self {
            ruleset_version: RULESET_VERSION.saturating_add(epoch),
            rules: compiled,
            questions,
        })
    }

    /// Reads the rule answers out of a Clef response. A missing or
    /// non-finite answer fires nothing and is not counted as applied.
    pub fn evaluate(&self, response: &ClefResponse) -> RuleOutcome {
        let mut outcome = RuleOutcome::default();
        for rule in &self.rules {
            let Some(probability) = response.noul_answer(&rule.key) else {
                continue;
            };
            if !probability.is_finite() {
                continue;
            }
            outcome.applied.push(rule.rule_id.clone());
            if probability >= rule.threshold {
                outcome.fired.push(FiredRule {
                    rule_id: rule.reason_tail(),
                    effect: rule.effect,
                });
                outcome.details.push((
                    rule.reason_tail(),
                    json!({
                        "rule_id": rule.rule_id,
                        "topic": rule.topic,
                        "question": rule.topic.spec().instructions,
                        "probability_true": probability,
                        "threshold": rule.threshold,
                    }),
                ));
            }
        }
        outcome
    }
}

/// The number in `SOLV-RULE-014`, for ordering. Ids without one sort last.
fn rule_number(rule_id: &str) -> u64 {
    rule_id
        .strip_prefix(RULE_ID_PREFIX)
        .and_then(|tail| tail.parse::<u64>().ok())
        .unwrap_or(u64::MAX)
}

/// `rule_014` from `SOLV-RULE-014`: only ASCII letters and digits of the
/// id's tail, so a hand-edited snapshot cannot smuggle odd characters into
/// a JSON key. `None` when nothing usable is left.
fn question_key(rule_id: &str) -> Option<String> {
    let tail = rule_id.strip_prefix(RULE_ID_PREFIX)?;
    let cleaned: String = tail
        .chars()
        .filter(char::is_ascii_alphanumeric)
        .take(MAX_KEY_CHARS)
        .collect::<String>()
        .to_ascii_lowercase();
    if cleaned.is_empty() {
        None
    } else {
        Some(format!("rule_{cleaned}"))
    }
}

/// The next rule id after the highest one in `rules`, or `None` on
/// overflow.
pub fn next_rule_id(rules: &[LearnedRule]) -> Option<String> {
    let highest = rules
        .iter()
        .filter_map(|rule| rule.rule_id.strip_prefix(RULE_ID_PREFIX))
        .filter_map(|tail| tail.parse::<u64>().ok())
        .max()
        .unwrap_or(0);
    let next = highest.checked_add(1)?;
    Some(format!("{RULE_ID_PREFIX}{next:03}"))
}

/// Replaces the evidence of each learned-rule reason with the rule's detail:
/// which rule and topic, the question Solv asked (catalogue wording), the
/// answer, and the threshold. The critique is never part of it.
pub fn attach_rule_detail(reasons: &mut [Reason], outcome: &RuleOutcome) {
    for reason in reasons {
        let ReasonCode::Rule(tail) = &reason.code else {
            continue;
        };
        let Some((_, detail)) = outcome.details.iter().find(|(id, _)| id == tail) else {
            continue;
        };
        if let Some(evidence) = reason.evidence.as_mut() {
            evidence.detector_output = Some(detail.clone());
        }
    }
}
