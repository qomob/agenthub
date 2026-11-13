//! Cost Optimizer rules engine (M2-A). A stateless, offline, explainable
//! rules engine over the data the user registered in My AI Stack.
//!
//! Design contract (COST_OPTIMIZER_PRD §0–§2, frozen at M0):
//! - pure functions, mirroring `validation.rs::derive_health`: input
//!   `(stack_items, subscriptions, config)`, output recommendations; no
//!   database access, no locks, no network, exhaustively unit-testable;
//! - it never invents facts: no usage evidence ⇒ never "idle/wasted"; no
//!   price catalogue ⇒ never a factual "cheaper plan" claim;
//! - recommendations are "review" prompts, never actions, and adoption has
//!   no real-world side effect;
//! - insufficient evidence degrades to a data-gap report (R5) instead of a
//!   low-quality recommendation;
//! - deterministic IDs (`rec_` + sha256 of rule inputs, 12 hex): the same
//!   inputs always yield byte-identical output, which the reproducibility
//!   test locks down.
//!
//! Savings semantics (PRD §3): everything this engine produces is
//! *potential* (simulated) or a prompt; there is no "verified" confidence
//! level and no realized-savings claim anywhere in this module.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::Result;
use crate::model::{StackItemView, SubscriptionView};
use crate::money::{self, BillingPeriod};

// ---------------------------------------------------------------------------
// Configuration
// ---------------------------------------------------------------------------

/// Rule parameters, all user-configurable. Anything left unset keeps the
/// corresponding rule silent — an unconfigured rule never guesses.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OptimizerConfig {
    /// R2: review threshold per currency in minor units (monthly equivalent
    /// at or above this figure is flagged). Empty map = R2 fully silent.
    pub cost_thresholds_minor: std::collections::BTreeMap<String, i64>,
    /// R3: renewal reminder window in days (PRD default 7, range 1–30).
    pub renewal_window_days: Option<i64>,
    /// Today's date for renewal arithmetic. `None` = system date. Exposed so
    /// tests can freeze time and keep output deterministic.
    pub today: Option<chrono::NaiveDate>,
}

impl OptimizerConfig {
    fn renewal_window(&self) -> i64 {
        self.renewal_window_days.unwrap_or(7).clamp(1, 30)
    }

    /// The engine never reads the system clock: renewal classification must
    /// replay identically for the same inputs regardless of when or where the
    /// report runs. Callers (the service layer) resolve the business date and
    /// pin it here; a missing date is an explicit input error.
    fn pinned_today(&self) -> Result<chrono::NaiveDate> {
        self.today.ok_or_else(|| {
            crate::error::Error::invalid_input(
                "optimizer config must pin today's date for deterministic replay",
            )
        })
    }
}

// ---------------------------------------------------------------------------
// Persisted rule parameters (M2-B): one storage format shared by GUI + CLI
// ---------------------------------------------------------------------------

/// The persisted, user-editable rule parameters. GUI and CLI both go through
/// the same service validation and the same storage keys, so both surfaces
/// always read one configuration.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct OptimizerSettings {
    /// R2 review thresholds per currency in minor units. Empty = R2 silent.
    pub cost_thresholds_minor: std::collections::BTreeMap<String, i64>,
    /// R3 window in days; None = engine default (7). Valid range 1..=30.
    pub renewal_window_days: Option<i64>,
}

/// Storage keys — a single namespace shared by every front end.
pub const SETTINGS_KEY_THRESHOLDS: &str = "optimizer.cost_thresholds_minor";
pub const SETTINGS_KEY_WINDOW: &str = "optimizer.renewal_window_days";

impl OptimizerSettings {
    /// Parse from the two storage keys. Unknown/invalid values are reported
    /// as invalid input instead of being silently dropped.
    pub fn load(get: &dyn Fn(&str) -> crate::error::Result<Option<String>>) -> Result<Self> {
        let thresholds = match get(SETTINGS_KEY_THRESHOLDS)? {
            None => Default::default(),
            Some(raw) => serde_json::from_str::<std::collections::BTreeMap<String, i64>>(&raw)
                .map_err(|e| {
                    crate::error::Error::invalid_input(format!(
                        "stored optimizer thresholds are not valid JSON: {e}"
                    ))
                })?,
        };
        let window = match get(SETTINGS_KEY_WINDOW)? {
            // Empty string is the stored encoding of "cleared to default".
            None => None,
            Some(raw) if raw.trim().is_empty() => None,
            Some(raw) => Some(raw.trim().parse::<i64>().map_err(|_| {
                crate::error::Error::invalid_input(format!(
                    "stored optimizer renewal window {raw:?} is not an integer"
                ))
            })?),
        };
        let settings = Self {
            cost_thresholds_minor: thresholds,
            renewal_window_days: window,
        };
        settings.validate()?;
        Ok(settings)
    }

    /// Validate every field. The same rules run on read (corrupt storage is
    /// an error, not a silent fallback) and on write (the user cannot store
    /// anything the engine would reject).
    pub fn validate(&self) -> Result<()> {
        for (currency, threshold) in &self.cost_thresholds_minor {
            money::validate_currency(currency)?;
            money::validate_amount_minor(*threshold)?;
            if *threshold <= 0 {
                return Err(crate::error::Error::invalid_input(format!(
                    "optimizer threshold for {currency} must be positive, got {threshold}"
                )));
            }
        }
        if let Some(window) = self.renewal_window_days {
            if !(1..=30).contains(&window) {
                return Err(crate::error::Error::invalid_input(format!(
                    "optimizer renewal window must be 1-30 days, got {window}"
                )));
            }
        }
        Ok(())
    }
}

// ---------------------------------------------------------------------------
// Output contract
// ---------------------------------------------------------------------------

/// Evidence attached to a recommendation (PRD §2 `Evidence`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Evidence {
    /// user_input | detection | rule_param
    pub kind: String,
    pub description: String,
    pub source_ref: String,
}

/// Simulated money delta for a single currency (PRD §2 `MoneyDelta`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoneyDelta {
    pub currency: String,
    pub minor_before: i64,
    pub minor_after: i64,
    pub delta_minor: i64,
    /// "monthly" | "annual"
    pub period: String,
}

/// One optimizer recommendation (PRD §2 `Recommendation`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Recommendation {
    /// Deterministic: `rec_` + first 12 hex of sha256 over the rule inputs.
    pub recommendation_id: String,
    /// R1_OVERLAP | R2_HIGH_COST | R3_RENEWAL | R4_SCENARIO | R5_DATA_GAP
    pub recommendation_type: String,
    pub title: String,
    /// Human explanation naming the rule and its trigger condition.
    pub explanation: String,
    pub related_stack_items: Vec<String>,
    pub related_subscriptions: Vec<String>,
    pub evidence: Vec<Evidence>,
    /// Non-empty when the recommendation was degraded for missing facts.
    pub missing_information: Vec<String>,
    /// Stated assumptions, e.g. "annual folded by ×12 to a monthly equivalent".
    pub assumptions: Vec<String>,
    /// Present only when a single-currency arithmetic delta can be stated.
    pub estimated_impact: Option<MoneyDelta>,
    pub currency: Option<String>,
    /// input_only | user_configured | derived_arithmetic — never "verified".
    pub confidence_or_evidence_level: String,
    /// review | complete_data | none
    pub user_action_required: String,
}

/// Full read-only report (PRD §4 `optimizer_report`).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerReport {
    pub generated_at: String,
    pub recommendations: Vec<Recommendation>,
    /// Counters mirrored from the spending overview so the caller can see
    /// what the engine saw without re-reading the database.
    pub counts: ReportCounts,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ReportCounts {
    pub stack_items: i64,
    pub subscriptions_total: i64,
    pub subscriptions_active: i64,
    pub recommendations: i64,
    pub data_gaps: i64,
}

// ---------------------------------------------------------------------------
// Deterministic ID
// ---------------------------------------------------------------------------

fn deterministic_id(
    rule: &str,
    items: &[String],
    subs: &[String],
    params: &str,
) -> String {
    let mut hasher = Sha256::new();
    hasher.update(rule.as_bytes());
    hasher.update(b"|items|");
    let mut items: Vec<&str> = items.iter().map(String::as_str).collect();
    items.sort_unstable();
    hasher.update(items.join(",").into_bytes());
    hasher.update(b"|subs|");
    let mut subs: Vec<&str> = subs.iter().map(String::as_str).collect();
    subs.sort_unstable();
    hasher.update(subs.join(",").into_bytes());
    hasher.update(b"|params|");
    hasher.update(params.as_bytes());
    let digest = hasher.finalize();
    format!("rec_{}", hex::encode(&digest[..6]))
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// The active subscriptions grouped by their stack item id, preserving input
/// order within each group (BTreeMap keeps the report stable across runs).
/// Monthly-equivalent fold for one subscription, `None` when it cannot fold
/// (missing amount/currency/period, usage without bills, unknown type).
/// Hybrid subscriptions contribute their known fixed fee — the caller must
/// surface the "floating part not counted" assumption.
/// The foldable core of one subscription: currency, fixed-fee amount and
/// billing period — or `None` when the record cannot fold.
///
/// This is the eligibility gate for the money arithmetic (R2/R4). The R5 gap
/// explanations use `data_gaps()`, a parallel view with the same match arms;
/// the invariant that the two views agree is locked by
/// `fold_eligibility_and_gap_explanations_stay_in_lockstep`:
/// - only active records participate;
/// - `usage` amounts are metered, never folded — even when amount, currency
///   and period are all present;
/// - `free` folds as a known zero (amount 0, annual-equivalent period);
/// - `unknown` billing types cannot fold.
fn foldable_parts(sub: &SubscriptionView) -> Option<(String, i64, BillingPeriod)> {
    if sub.status != "active" {
        return None;
    }
    match sub.billing_type.as_str() {
        "free" => {
            return sub.currency.as_ref().map(|c| {
                (c.clone(), 0, BillingPeriod::parse("yearly").expect("yearly exists"))
            });
        }
        "usage" | "unknown" => return None,
        _ => {}
    }
    let amount = sub.amount_minor?;
    let currency = sub.currency.clone()?;
    let period = BillingPeriod::parse(sub.billing_period.as_deref()?)?;
    Some((currency, amount, period))
}

/// Monthly-equivalent convenience over [`foldable_parts`].
fn monthly_equivalent_of(sub: &SubscriptionView) -> Option<(String, i64)> {
    let (currency, amount, period) = foldable_parts(sub)?;
    let monthly = money::monthly_equivalent(amount, period).ok()?;
    Some((currency, monthly))
}

/// Data gaps for one subscription, mirroring the spending overview rules so
/// R5 and the cost card can never disagree about what is missing.
fn data_gaps(sub: &SubscriptionView) -> Vec<String> {
    let mut gaps = Vec::new();
    match sub.billing_type.as_str() {
        "free" => {}
        "usage" => {
            if sub.amount_minor.is_some() {
                gaps.push("usage_amount_not_foldable".to_string());
            }
        }
        "unknown" => gaps.push("billing_type".to_string()),
        "subscription" | "hybrid" => {
            if sub.amount_minor.is_none() {
                gaps.push("amount".to_string());
            }
            if sub
                .currency
                .as_deref()
                .map(str::trim)
                .filter(|c| !c.is_empty())
                .is_none()
            {
                gaps.push("currency".to_string());
            }
            if BillingPeriod::parse(sub.billing_period.as_deref().unwrap_or_default()).is_none() {
                gaps.push("billing_period".to_string());
            }
        }
        _ => gaps.push("billing_type".to_string()),
    }
    gaps
}

// ---------------------------------------------------------------------------
// R1 — potential overlap within a user-declared group
// ---------------------------------------------------------------------------

fn rule_r1_overlap(
    items: &[StackItemView],
    subscriptions: &[SubscriptionView],
    today: chrono::NaiveDate,
) -> Vec<Recommendation> {
    let mut out = Vec::new();
    // Group distinct unmerged items by overlap_group. The engine performs no
    // semantic inference: the group is whatever the user typed.
    let mut groups: std::collections::BTreeMap<&str, Vec<&StackItemView>> =
        std::collections::BTreeMap::new();
    for item in items {
        if item.merged_into.is_some() {
            continue;
        }
        if let Some(group) = item.overlap_group.as_deref().map(str::trim) {
            if !group.is_empty() {
                groups.entry(group).or_default().push(item);
            }
        }
    }
    for (group, members) in groups {
        if members.len() < 2 {
            continue;
        }
        let mut related_items = Vec::new();
        let mut related_subs = Vec::new();
        let mut lines = Vec::new();
        for item in &members {
            related_items.push(item.id.clone());
            for sub in subscriptions
                .iter()
                .filter(|s| s.stack_item_id == item.id && s.status == "active")
            {
                related_subs.push(sub.id.clone());
                match monthly_equivalent_of(sub) {
                    Some((currency, monthly)) => lines.push(format!(
                        "{} · {} · {}.00 {}/month (monthly equivalent)",
                        item.name, sub.product_name, monthly, currency
                    )),
                    None => lines.push(format!(
                        "{} · {} · monthly equivalent unavailable (incomplete facts)",
                        item.name, sub.product_name
                    )),
                }
            }
            lines.push(format!(
                "{} · registered {}",
                item.name,
                item.created_at.split('T').next().unwrap_or(&item.created_at)
            ));
        }
        // Merged-group subtlety (P0-2/I5): two different subscription records
        // inside one merge group are never automatically double charging, so
        // this stays a review prompt, never a financial claim.
        let sub_note = if related_subs.len() >= 2 {
            format!(
                "The group holds {} subscription records; they are not automatically treated as duplicate charges.",
                related_subs.len()
            )
        } else {
            "The group currently holds one subscription record.".to_string()
        };
        out.push(Recommendation {
            recommendation_id: deterministic_id(
                "R1_OVERLAP",
                &related_items,
                &related_subs,
                group,
            ),
            recommendation_type: "R1_OVERLAP".to_string(),
            related_stack_items: related_items.clone(),
            related_subscriptions: related_subs.clone(),
            title: format!("Potential overlap in group \"{group}\""),
            explanation: format!(
                "You placed {} tools in the same overlap group \"{}\". Tools in one group may serve a similar purpose; review whether you still need all of their subscriptions. {}",
                members.len(),
                group,
                sub_note
            ),
            evidence: vec![
                Evidence {
                    kind: "user_input".to_string(),
                    description: format!("overlap_group=\"{group}\" set on {} tools", members.len()),
                    source_ref: "stack_items.overlap_group".to_string(),
                },
                Evidence {
                    kind: "rule_param".to_string(),
                    description: format!("group reviewed as of {}", today.to_string()),
                    source_ref: "optimizer.r1".to_string(),
                },
            ],
            missing_information: Vec::new(),
            assumptions: Vec::new(),
            estimated_impact: None,
            currency: None,
            confidence_or_evidence_level: "input_only".to_string(),
            user_action_required: "review".to_string(),
        });
        // Attach the per-tool line list as explanation detail, sorted for
        // byte-stable output.
        if let Some(rec) = out.last_mut() {
            lines.sort();
            rec.assumptions = vec!["Monthly equivalents are folded estimates, not this month's charge.".to_string()];
            rec.explanation.push_str(&format!("\n{}", lines.join("\n")));
        }
    }
    out
}

// ---------------------------------------------------------------------------
// R2 — high-cost review against a user-set threshold
// ---------------------------------------------------------------------------

fn rule_r2_high_cost(
    subscriptions: &[SubscriptionView],
    config: &OptimizerConfig,
) -> Vec<Recommendation> {
    let mut out = Vec::new();
    if config.cost_thresholds_minor.is_empty() {
        return out; // no thresholds configured: the rule is silent
    }
    for sub in subscriptions {
        if sub.status != "active" {
            continue;
        }
        let Some((currency, monthly)) = monthly_equivalent_of(sub) else {
            continue; // R5 covers the missing facts
        };
        let Some(&threshold) = config.cost_thresholds_minor.get(&currency) else {
            continue;
        };
        if monthly < threshold {
            continue;
        }
        let hybrid_note = if sub.billing_type == "hybrid" {
            " Hybrid subscriptions compare their known fixed fee only; the floating part is not counted."
        } else {
            ""
        };
        out.push(Recommendation {
            recommendation_id: deterministic_id(
                "R2_HIGH_COST",
                &[sub.stack_item_id.clone()],
                &[sub.id.clone()],
                &format!("{currency}:{threshold}"),
            ),
            recommendation_type: "R2_HIGH_COST".to_string(),
            title: format!(
                "{sub_product} is at or above your {currency} review threshold",
                sub_product = sub.product_name
            ),
            explanation: format!(
                "The monthly equivalent of \"{product}\" is {monthly_whole}.{monthly_frac:0>2} {currency}, at or above the {threshold_whole}.{threshold_frac:0>2} {currency} review threshold you configured. This is a prompt to review the subscription, not a claim that it is wasteful.{hybrid}",
                product = sub.product_name,
                monthly_whole = monthly / 100,
                monthly_frac = (monthly % 100).abs(),
                threshold_whole = threshold / 100,
                threshold_frac = (threshold % 100).abs(),
                hybrid = hybrid_note,
            ),
            related_stack_items: vec![sub.stack_item_id.clone()],
            related_subscriptions: vec![sub.id.clone()],
            evidence: vec![
                Evidence {
                    kind: "user_input".to_string(),
                    description: format!(
                        "registered {amount} minor per {period}",
                        amount = sub.amount_minor.unwrap_or_default(),
                        period = sub.billing_period.as_deref().unwrap_or("unknown"),
                    ),
                    source_ref: format!("subscription_records.{}", sub.id),
                },
                Evidence {
                    kind: "rule_param".to_string(),
                    description: format!("cost threshold {threshold} minor per {currency}"),
                    source_ref: "optimizer.r2".to_string(),
                },
            ],
            missing_information: Vec::new(),
            assumptions: vec![
                "Monthly equivalent is a folded estimate, not this month's charge.".to_string(),
            ],
            estimated_impact: None,
            currency: Some(currency),
            confidence_or_evidence_level: "user_configured".to_string(),
            user_action_required: "review".to_string(),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// R3 — renewal reminders
// ---------------------------------------------------------------------------

fn rule_r3_renewals(
    subscriptions: &[SubscriptionView],
    config: &OptimizerConfig,
) -> Result<Vec<Recommendation>> {
    let today = config.pinned_today()?;
    let window = config.renewal_window();
    let window_end = today
        .checked_add_signed(chrono::Duration::days(window))
        .expect("renewal window within date range");

    let mut upcoming: Vec<(chrono::NaiveDate, &SubscriptionView)> = Vec::new();
    let mut stale: Vec<(chrono::NaiveDate, &SubscriptionView)> = Vec::new();
    for sub in subscriptions {
        if sub.status != "active" {
            continue;
        }
        if let Some(date) = sub
            .renews_on
            .as_deref()
            .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        {
            if date >= today && date <= window_end {
                upcoming.push((date, sub));
            } else if date < today {
                stale.push((date, sub));
            }
        }
    }
    upcoming.sort_by_key(|(d, _)| *d);
    stale.sort_by_key(|(d, _)| *d);

    let mut out = Vec::new();
    if !upcoming.is_empty() {
        let related_subs: Vec<String> =
            upcoming.iter().map(|(_, s)| s.id.clone()).collect();
        let related_items: Vec<String> = upcoming
            .iter()
            .map(|(_, s)| s.stack_item_id.clone())
            .collect();
        let lines: Vec<String> = upcoming
            .iter()
            .map(|(d, s)| {
                format!(
                    "{} · {} · renews {} (in {} day(s))",
                    s.product_name,
                    s.currency.clone().unwrap_or_else(|| "unknown".into()),
                    d.to_string(),
                    (*d - today).num_days()
                )
            })
            .collect();
        out.push(Recommendation {
            recommendation_id: deterministic_id(
                "R3_RENEWAL_UPCOMING",
                &related_items,
                &related_subs,
                &format!("window:{window}:today:{today}"),
            ),
            recommendation_type: "R3_RENEWAL".to_string(),
            related_stack_items: related_items.clone(),
            related_subscriptions: related_subs.clone(),
            title: format!(
                "{} subscription(s) renew within {} day(s)",
                upcoming.len(),
                window
            ),
            explanation: format!(
                "These subscriptions have a renewal date within your {}-day reminder window:\n{}",
                window,
                lines.join("\n")
            ),
            evidence: vec![Evidence {
                kind: "user_input".to_string(),
                description: "renews_on dates registered on the subscriptions".to_string(),
                source_ref: "subscription_records.renews_on".to_string(),
            }],
            missing_information: Vec::new(),
            assumptions: vec![format!("Window counted from {}", today.to_string())],
            estimated_impact: None,
            currency: None,
            confidence_or_evidence_level: "user_configured".to_string(),
            user_action_required: "review".to_string(),
        });
    }
    if !stale.is_empty() {
        let related_subs: Vec<String> = stale.iter().map(|(_, s)| s.id.clone()).collect();
        let related_items: Vec<String> = stale
            .iter()
            .map(|(_, s)| s.stack_item_id.clone())
            .collect();
        let lines: Vec<String> = stale
            .iter()
            .map(|(d, s)| {
                format!(
                    "{} · renewal date {} already passed but status is still active",
                    s.product_name,
                    d.to_string()
                )
            })
            .collect();
        out.push(Recommendation {
            recommendation_id: deterministic_id(
                "R3_RENEWAL_STALE",
                &related_items,
                &related_subs,
                &format!("stale:today:{today}"),
            ),
            recommendation_type: "R3_RENEWAL".to_string(),
            related_stack_items: related_items.clone(),
            related_subscriptions: related_subs.clone(),
            title: format!(
                "{} subscription(s) have a renewal date in the past but are still active",
                stale.len()
            ),
            explanation: format!(
                "These records look stale: the registered renewal date has passed while the subscription is still marked active. Update the record or its status:\n{}",
                lines.join("\n")
            ),
            evidence: vec![Evidence {
                kind: "user_input".to_string(),
                description: "renews_on in the past with status=active".to_string(),
                source_ref: "subscription_records.renews_on".to_string(),
            }],
            missing_information: Vec::new(),
            assumptions: Vec::new(),
            estimated_impact: None,
            currency: None,
            confidence_or_evidence_level: "input_only".to_string(),
            user_action_required: "complete_data".to_string(),
        });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// R5 — data gaps (the degradation path)
// ---------------------------------------------------------------------------

fn rule_r5_data_gaps(
    items: &[StackItemView],
    subscriptions: &[SubscriptionView],
    today: chrono::NaiveDate,
) -> Vec<Recommendation> {
    let name_of = |id: &str| {
        items
            .iter()
            .find(|i| i.id == id)
            .map(|i| i.name.clone())
            .unwrap_or_else(|| id.to_string())
    };
    // Gap buckets over active subscriptions.
    let mut buckets: std::collections::BTreeMap<String, Vec<String>> =
        std::collections::BTreeMap::new();
    let mut gap_sub_ids: Vec<String> = Vec::new();
    let mut ordered: Vec<&SubscriptionView> = subscriptions
        .iter()
        .filter(|s| s.status == "active")
        .collect();
    ordered.sort_by(|a, b| a.id.cmp(&b.id));
    for sub in ordered {
        let mut gaps = data_gaps(sub);
        if sub.renews_on.is_none() {
            gaps.push("renews_on".to_string());
        }
        if let Some(d) = sub
            .renews_on
            .as_deref()
            .and_then(|d| chrono::NaiveDate::parse_from_str(d, "%Y-%m-%d").ok())
        {
            if d < today {
                gaps.push("renewal_date_stale".to_string());
            }
        }
        // Never-confirmed item with subscriptions is a confidence gap too.
        if !gaps.is_empty() {
            gap_sub_ids.push(sub.id.clone());
            for gap in gaps {
                buckets
                    .entry(gap)
                    .or_default()
                    .push(format!("{} · {}", name_of(&sub.stack_item_id), sub.product_name));
            }
        }
    }
    // Item-level gap: holds subscriptions but the item itself was never confirmed.
    let unconfirmed_items: Vec<&StackItemView> = items
        .iter()
        .filter(|i| !i.confirmed && i.subscription_count > 0)
        .collect();

    let mut out = Vec::new();
    if !buckets.is_empty() {
        let mut sections: Vec<String> = Vec::new();
        for (gap, entries) in &buckets {
            let mut sorted_entries = entries.clone();
            sorted_entries.sort();
            sections.push(format!("[{}] {}", gap, sorted_entries.join("; ")));
        }
        sections.sort();
        let related_items: Vec<String> = items
            .iter()
            .filter(|i| {
                subscriptions
                    .iter()
                    .any(|s| s.stack_item_id == i.id && s.status == "active")
            })
            .map(|i| i.id.clone())
            .collect();
        let related_items_for_rec = related_items.clone();
        out.push(Recommendation {
            recommendation_id: deterministic_id(
                "R5_DATA_GAP",
                &related_items,
                &gap_sub_ids,
                &format!("today:{today}:buckets:{}", sections.join("|")),
            ),
            recommendation_type: "R5_DATA_GAP".to_string(),
            title: format!(
                "{} subscription record(s) are missing facts needed for cost analysis",
                gap_sub_ids.len()
            ),
            explanation: format!(
                "These subscriptions cannot take part in every rule until the facts are completed:\n{}",
                sections.join("\n")
            ),
            evidence: vec![Evidence {
                kind: "user_input".to_string(),
                description: "missing optional fields on subscription records".to_string(),
                source_ref: "subscription_records".to_string(),
            }],
            related_stack_items: related_items_for_rec,
            related_subscriptions: gap_sub_ids.clone(),
            missing_information: {
                let mut m: Vec<String> = buckets.keys().cloned().collect();
                m.sort();
                m
            },
            assumptions: Vec::new(),
            estimated_impact: None,
            currency: None,
            confidence_or_evidence_level: "input_only".to_string(),
            user_action_required: "complete_data".to_string(),
        });
    }
    if !unconfirmed_items.is_empty() {
        let related_items: Vec<String> =
            unconfirmed_items.iter().map(|i| i.id.clone()).collect();
        let mut names: Vec<String> = unconfirmed_items.iter().map(|i| i.name.clone()).collect();
        names.sort();
        out.push(Recommendation {
            recommendation_id: deterministic_id(
                "R5_UNCONFIRMED",
                &related_items,
                &[],
                &format!("today:{today}"),
            ),
            recommendation_type: "R5_DATA_GAP".to_string(),
            title: format!(
                "{} tool(s) hold subscriptions but have never been confirmed",
                unconfirmed_items.len()
            ),
            explanation: format!(
                "Confirming a tool records that its registered facts were reviewed by you; these tools still wait for that review:\n{}",
                names.join("\n")
            ),
            evidence: vec![Evidence {
                kind: "detection".to_string(),
                description: "stack_items.confirmed = false with active subscriptions".to_string(),
                source_ref: "stack_items.confirmed".to_string(),
            }],
            related_stack_items: related_items,
            related_subscriptions: Vec::new(),
            missing_information: vec!["item_confirmation".to_string()],
            assumptions: Vec::new(),
            estimated_impact: None,
            currency: None,
            confidence_or_evidence_level: "input_only".to_string(),
            user_action_required: "complete_data".to_string(),
        });
    }
    out
}

// ---------------------------------------------------------------------------
// R4 — scenario simulation (pure arithmetic, nothing persisted)
// ---------------------------------------------------------------------------

/// One simulated change (PRD §R4). The engine only does arithmetic on
/// figures the user already registered or supplies here.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
/// One simulated change (PRD §R4). Wire format is part of the frozen GUI/CLI
/// contract: `tag = "kind"` names the variant and every variant body field is
/// camelCase — `rename_all` on the enum alone renames variant NAMES but NOT
/// variant body FIELDS, which is exactly the mismatch the M2-B review caught.
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum ScenarioAction {
    /// Cancel one subscription entirely.
    #[serde(rename_all = "camelCase")]
    Cancel {
        subscription_id: String,
    },
    /// Lower the amount of a known subscription (same currency/period).
    #[serde(rename_all = "camelCase")]
    Reduce {
        subscription_id: String,
        new_amount_minor: i64,
    },
    /// Swap to a user-supplied alternative plan (new amount/currency/period).
    #[serde(rename_all = "camelCase")]
    Replace {
        subscription_id: String,
        new_amount_minor: i64,
        new_currency: String,
        new_period: String,
    },
    /// Deactivate every subscription registered on one tool.
    #[serde(rename_all = "camelCase")]
    DeactivateItem { stack_item_id: String },
}

/// Result of one scenario: per-currency before/after equivalents plus the
/// affected subscriptions. Nothing here is persisted (PRD: potential only).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptimizerScenarioResult {
    pub generated_at: String,
    pub actions: Vec<ScenarioAction>,
    pub affected_subscriptions: Vec<String>,
    /// Per-currency monthly equivalents before and after, sorted by currency.
    pub deltas_monthly: Vec<MoneyDelta>,
    pub deltas_annual: Vec<MoneyDelta>,
    pub notes: Vec<String>,
}

fn apply_actions(
    subscriptions: &[SubscriptionView],
    actions: &[ScenarioAction],
) -> Result<(Vec<SubscriptionView>, Vec<String>)> {
    let mut rows: Vec<SubscriptionView> = subscriptions.to_vec();
    let mut affected = Vec::new();
    for action in actions {
        match action {
            ScenarioAction::Cancel { subscription_id } => {
                let sub = rows
                    .iter_mut()
                    .find(|s| &s.id == subscription_id)
                    .ok_or_else(|| {
                        crate::error::Error::invalid_input(format!(
                            "scenario references unknown subscription {subscription_id}"
                        ))
                    })?;
                if sub.status == "active" {
                    sub.status = "cancelled".to_string();
                    affected.push(sub.id.clone());
                }
            }
            ScenarioAction::Reduce {
                subscription_id,
                new_amount_minor,
            } => {
                money::validate_amount_minor(*new_amount_minor)?;
                let sub = rows
                    .iter_mut()
                    .find(|s| &s.id == subscription_id)
                    .ok_or_else(|| {
                        crate::error::Error::invalid_input(format!(
                            "scenario references unknown subscription {subscription_id}"
                        ))
                    })?;
                if sub.amount_minor.is_none() {
                    return Err(crate::error::Error::invalid_input(format!(
                        "subscription {subscription_id} has no known amount to reduce"
                    )));
                }
                sub.amount_minor = Some(*new_amount_minor);
                affected.push(sub.id.clone());
            }
            ScenarioAction::Replace {
                subscription_id,
                new_amount_minor,
                new_currency,
                new_period,
            } => {
                money::validate_amount_minor(*new_amount_minor)?;
                money::validate_currency(new_currency)?;
                let period = BillingPeriod::parse(new_period.as_str()).ok_or_else(|| {
                    crate::error::Error::invalid_input(format!(
                        "unknown billing period {new_period:?}"
                    ))
                })?;
                let _ = period;
                let sub = rows
                    .iter_mut()
                    .find(|s| &s.id == subscription_id)
                    .ok_or_else(|| {
                        crate::error::Error::invalid_input(format!(
                            "scenario references unknown subscription {subscription_id}"
                        ))
                    })?;
                sub.amount_minor = Some(*new_amount_minor);
                sub.currency = Some(new_currency.clone());
                sub.billing_period = Some(new_period.clone());
                affected.push(sub.id.clone());
            }
            ScenarioAction::DeactivateItem { stack_item_id } => {
                if !subscriptions.iter().any(|s| s.stack_item_id == *stack_item_id) {
                    return Err(crate::error::Error::invalid_input(format!(
                        "scenario references unknown stack item {stack_item_id}"
                    )));
                }
                for sub in rows
                    .iter_mut()
                    .filter(|s| s.stack_item_id == *stack_item_id && s.status == "active")
                {
                    sub.status = "cancelled".to_string();
                    affected.push(sub.id.clone());
                }
            }
        }
    }
    affected.sort();
    affected.dedup();
    Ok((rows, affected))
}

fn deltas_between(
    before: &[SubscriptionView],
    after: &[SubscriptionView],
) -> Result<(Vec<MoneyDelta>, Vec<MoneyDelta>)> {
    /// Fold through the SAME eligibility gate as every other rule
    /// (`foldable_parts`) and the SAME arithmetic as the spending overview
    /// (`money::aggregate_currency`), so a simulation can never count a
    /// record the cost card or R5 would refuse to fold (review P1).
    fn fold(
        subs: &[SubscriptionView],
    ) -> Result<std::collections::BTreeMap<String, money::CurrencyTotal>> {
        let mut groups: std::collections::BTreeMap<String, Vec<money::CostLine>> =
            std::collections::BTreeMap::new();
        for sub in subs {
            if let Some((currency, amount, period)) = foldable_parts(sub) {
                groups.entry(currency).or_default().push(money::CostLine {
                    amount_minor: amount,
                    period,
                });
            }
        }
        groups
            .into_iter()
            .map(|(currency, lines)| {
                let total = money::aggregate_currency(&lines)?;
                Ok((currency, total))
            })
            .collect()
    }
    let before = fold(before)?;
    let after = fold(after)?;
    let mut monthly = Vec::new();
    let mut annual = Vec::new();
    for currency in before.keys().chain(after.keys()).collect::<std::collections::BTreeSet<_>>() {
        let currency: &String = currency;
        let before_total = before.get(currency).copied();
        let after_total = after.get(currency).copied();
        let (b_m, b_a) = match before_total {
            Some(t) => (t.monthly_equivalent_minor, t.annual_equivalent_minor),
            None => (0, 0),
        };
        let (a_m, a_a) = match after_total {
            Some(t) => (t.monthly_equivalent_minor, t.annual_equivalent_minor),
            None => (0, 0),
        };
        if (b_m, b_a) == (a_m, a_a) {
            continue;
        }
        monthly.push(MoneyDelta {
            currency: (*currency).clone(),
            minor_before: b_m,
            minor_after: a_m,
            delta_minor: a_m - b_m,
            period: "monthly".to_string(),
        });
        annual.push(MoneyDelta {
            currency: (*currency).clone(),
            minor_before: b_a,
            minor_after: a_a,
            delta_minor: a_a - b_a,
            period: "annual".to_string(),
        });
    }
    Ok((monthly, annual))
}

/// Simulate the given actions against a snapshot. Pure arithmetic over
/// user-supplied figures; nothing touches the database and no "realized
/// savings" concept exists here (PRD §3).
pub fn simulate(
    items: &[StackItemView],
    subscriptions: &[SubscriptionView],
    actions: &[ScenarioAction],
    _config: &OptimizerConfig,
    now: &str,
) -> Result<OptimizerScenarioResult> {
    let (after_rows, affected) = apply_actions(subscriptions, actions)?;
    let (monthly, annual) = deltas_between(subscriptions, &after_rows)?;
    let mut notes = vec![
        "This is a simulation only: no subscription was changed and no money was saved yet.".to_string(),
        "Currencies are never summed together.".to_string(),
    ];
    // Gap notes share the SAME fold-eligibility gate as the arithmetic
    // (review P1) AND are scoped per state (review round 2, P1): a Replace
    // can fill in missing facts, so a record excluded from the BEFORE totals
    // may well be counted in the AFTER totals. Each state gets its own,
    // exact exclusion list — a blanket claim would contradict the figures.
    fn exclusion_note(
        label: &str,
        subs: &[SubscriptionView],
    ) -> Option<String> {
        let mut gaps: Vec<String> = Vec::new();
        for sub in subs {
            if sub.status != "active" {
                continue;
            }
            for gap in data_gaps(sub) {
                gaps.push(format!("{} ({}): {}", sub.id, sub.product_name, gap));
            }
        }
        gaps.sort();
        gaps.dedup();
        if gaps.is_empty() {
            None
        } else {
            Some(format!(
                "Excluded from the {} totals (incomplete facts): {}",
                label,
                gaps.join("; ")
            ))
        }
    }
    if let Some(note) = exclusion_note("BEFORE", subscriptions) {
        notes.push(note);
    }
    if let Some(note) = exclusion_note("AFTER", &after_rows) {
        notes.push(note);
    }
    if !affected.is_empty() {
        notes.push(format!(
            "Affected subscriptions: {}",
            affected.join(", ")
        ));
    }
    // Stable recommendation-style record of the scenario itself.
    let item_ids: Vec<String> = actions
        .iter()
        .filter_map(|a| match a {
            ScenarioAction::DeactivateItem { stack_item_id } => Some(stack_item_id.clone()),
            _ => None,
        })
        .collect();
    let sub_ids: Vec<String> = actions
        .iter()
        .filter_map(|a| match a {
            ScenarioAction::Cancel { subscription_id }
            | ScenarioAction::Reduce { subscription_id, .. }
            | ScenarioAction::Replace { subscription_id, .. } => {
                Some(subscription_id.clone())
            }
            ScenarioAction::DeactivateItem { .. } => None,
        })
        .collect();
    let _ = deterministic_id("R4_SCENARIO", &item_ids, &sub_ids, "scenario");
    let _ = items;
    Ok(OptimizerScenarioResult {
        generated_at: now.to_string(),
        actions: actions.to_vec(),
        affected_subscriptions: affected,
        deltas_monthly: monthly,
        deltas_annual: annual,
        notes,
    })
}

// ---------------------------------------------------------------------------
// Report entry point
// ---------------------------------------------------------------------------

/// Build the full read-only report. `now` is supplied by the caller so the
/// service layer can stamp its own timestamp while the rules stay pure.
pub fn build_report(
    items: &[StackItemView],
    subscriptions: &[SubscriptionView],
    config: &OptimizerConfig,
    now: &str,
) -> Result<OptimizerReport> {
    let today = config.pinned_today()?;
    let mut recommendations = Vec::new();
    recommendations.extend(rule_r1_overlap(items, subscriptions, today));
    recommendations.extend(rule_r2_high_cost(subscriptions, config));
    recommendations.extend(rule_r3_renewals(subscriptions, config)?);
    recommendations.extend(rule_r5_data_gaps(items, subscriptions, today));
    // Deterministic ordering: by type, then ID.
    recommendations.sort_by(|a, b| {
        a.recommendation_type
            .cmp(&b.recommendation_type)
            .then_with(|| a.recommendation_id.cmp(&b.recommendation_id))
    });
    let counts = ReportCounts {
        stack_items: items.len() as i64,
        subscriptions_total: subscriptions.len() as i64,
        subscriptions_active: subscriptions.iter().filter(|s| s.status == "active").count()
            as i64,
        recommendations: recommendations.len() as i64,
        data_gaps: recommendations
            .iter()
            .filter(|r| r.recommendation_type == "R5_DATA_GAP")
            .count() as i64,
    };
    Ok(OptimizerReport {
        generated_at: now.to_string(),
        recommendations,
        counts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::Value;

    fn item(id: &str, name: &str, overlap: Option<&str>) -> StackItemView {
        StackItemView {
            id: id.to_string(),
            name: name.to_string(),
            vendor: None,
            source: "manual".to_string(),
            detection_state: "unknown".to_string(),
            confirmed: true,
            last_confirmed_at: None,
            subscription_state: "registered".to_string(),
            merged_into: None,
            overlap_group: overlap.map(str::to_string),
            notes: None,
            created_at: "2026-11-01T00:00:00Z".to_string(),
            updated_at: "2026-11-01T00:00:00Z".to_string(),
            detection_count: 0,
            subscription_count: 0,
        }
    }

    #[allow(clippy::too_many_arguments)]
    fn sub(
        id: &str,
        item_id: &str,
        product: &str,
        billing_type: &str,
        amount: Option<i64>,
        currency: Option<&str>,
        period: Option<&str>,
        renews_on: Option<&str>,
    ) -> SubscriptionView {
        SubscriptionView {
            id: id.to_string(),
            stack_item_id: item_id.to_string(),
            product_name: product.to_string(),
            vendor: None,
            plan_name: None,
            billing_type: billing_type.to_string(),
            amount_minor: amount,
            currency: currency.map(str::to_string),
            billing_period: period.map(str::to_string),
            started_on: None,
            renews_on: renews_on.map(str::to_string),
            status: "active".to_string(),
            notes: None,
            info_source: None,
            last_confirmed_at: None,
            superseded_by: None,
            created_at: "2026-11-01T00:00:00Z".to_string(),
            updated_at: "2026-11-01T00:00:00Z".to_string(),
        }
    }

    fn config() -> OptimizerConfig {
        OptimizerConfig {
            today: Some(chrono::NaiveDate::from_ymd_opt(2026, 11, 12).unwrap()),
            ..Default::default()
        }
    }

    #[test]
    fn r1_stays_silent_without_overlap_groups() {
        let items = vec![item("a", "Alpha", None), item("b", "Beta", None)];
        let subs = vec![sub(
            "s1", "a", "Plan A", "subscription", Some(2000), Some("USD"),
            Some("monthly"), None,
        )];
        let report = build_report(&items, &subs, &config(), "2026-11-12T00:00:00Z").unwrap();
        assert!(
            report.recommendations.iter().all(|r| r.recommendation_type != "R1_OVERLAP"),
            "R1 must stay silent when no overlap_group is maintained"
        );
    }

    #[test]
    fn r1_prompts_for_two_items_in_one_group_without_duplicate_claim() {
        let items = vec![
            item("a", "Alpha", Some("coding")),
            item("b", "Beta", Some("coding")),
        ];
        let subs = vec![
            sub("s1", "a", "Plan A", "subscription", Some(2000), Some("USD"), Some("monthly"), None),
            sub("s2", "b", "Plan B", "subscription", Some(3000), Some("USD"), Some("monthly"), None),
        ];
        let report = build_report(&items, &subs, &config(), "2026-11-12T00:00:00Z").unwrap();
        let recs: Vec<_> = report
            .recommendations
            .iter()
            .filter(|r| r.recommendation_type == "R1_OVERLAP")
            .collect();
        assert_eq!(recs.len(), 1);
        let rec = recs[0];
        assert!(rec.title.contains("Potential overlap"));
        assert!(!rec.explanation.to_lowercase().contains("double charge"));
        assert!(!rec.explanation.to_lowercase().contains("replaceable"));
        assert!(rec.explanation.contains("not automatically treated as duplicate charges"));
        assert_eq!(rec.confidence_or_evidence_level, "input_only");
    }

    #[test]
    fn r2_silent_without_thresholds_and_fires_at_or_above() {
        let items = vec![item("a", "Alpha", None)];
        let subs = vec![sub(
            "s1", "a", "Pro", "subscription", Some(6000), Some("USD"), Some("monthly"), None,
        )];
        // No thresholds: silent.
        let empty = build_report(&items, &subs, &config(), "now").unwrap();
        assert!(empty.recommendations.iter().all(|r| r.recommendation_type != "R2_HIGH_COST"));
        // Threshold below the monthly equivalent: fires.
        let mut cfg = config();
        cfg.cost_thresholds_minor.insert("USD".into(), 5000);
        let report = build_report(&items, &subs, &cfg, "now").unwrap();
        let rec = report
            .recommendations
            .iter()
            .find(|r| r.recommendation_type == "R2_HIGH_COST")
            .expect("R2 should fire at threshold");
        assert!(rec.explanation.contains("60.00 USD"), "explanation={}", rec.explanation);
        assert_eq!(rec.confidence_or_evidence_level, "user_configured");
    }

    #[test]
    fn r2_ignores_hybrid_floating_part_and_unknowns() {
        let items = vec![item("a", "Alpha", None), item("b", "Beta", None)];
        let subs = vec![
            sub("s1", "a", "Plus", "hybrid", Some(3000), Some("USD"), Some("yearly"), None),
            sub("s2", "b", "Mystery", "usage", Some(9000), Some("USD"), Some("monthly"), None),
        ];
        let mut cfg = config();
        cfg.cost_thresholds_minor.insert("USD".into(), 200);
        let report = build_report(&items, &subs, &cfg, "now").unwrap();
        let r2: Vec<_> = report
            .recommendations
            .iter()
            .filter(|r| r.recommendation_type == "R2_HIGH_COST")
            .collect();
        // Hybrid folds 3000/year = 250/month >= 200 → fires with the hybrid note.
        assert_eq!(r2.len(), 1);
        assert!(r2[0].explanation.contains("floating part is not counted"));
        // The usage line with an amount must not produce an R2 (it lands in R5).
        assert!(r2[0].related_subscriptions == vec!["s1".to_string()]);
    }

    #[test]
    fn r3_lists_upcoming_and_stale_separately() {
        let items = vec![item("a", "Alpha", None), item("b", "Beta", None)];
        let subs = vec![
            sub("s1", "a", "Soon", "subscription", Some(1000), Some("USD"), Some("monthly"), Some("2026-11-15")),
            sub("s2", "b", "Stale", "subscription", Some(1000), Some("USD"), Some("monthly"), Some("2026-10-01")),
        ];
        let report = build_report(&items, &subs, &config(), "now").unwrap();
        let r3: Vec<_> = report
            .recommendations
            .iter()
            .filter(|r| r.recommendation_type == "R3_RENEWAL")
            .collect();
        assert_eq!(r3.len(), 2, "upcoming and stale must be separate records");
        assert!(r3.iter().any(|r| r.title.contains("renew within")));
        let stale = r3.iter().find(|r| r.title.contains("past")).unwrap();
        assert_eq!(stale.user_action_required, "complete_data");
    }

    #[test]
    fn r5_buckets_every_gap_type() {
        let items = vec![item("a", "Alpha", None), item("b", "Beta", None)];
        let subs = vec![
            sub("s1", "a", "NoAmount", "subscription", None, Some("USD"), Some("monthly"), None),
            sub("s2", "b", "Stale", "subscription", Some(100), Some("USD"), Some("monthly"), Some("2026-10-01")),
        ];
        let report = build_report(&items, &subs, &config(), "now").unwrap();
        let gaps: Vec<_> = report
            .recommendations
            .iter()
            .filter(|r| r.recommendation_type == "R5_DATA_GAP")
            .collect();
        assert!(gaps.iter().any(|r| r.missing_information.contains(&"amount".to_string())));
        assert!(gaps.iter().any(|r| r.missing_information.contains(&"renewal_date_stale".to_string())));
        assert!(gaps.iter().all(|r| r.user_action_required == "complete_data"));
    }

    #[test]
    fn r5_flags_item_with_subscriptions_never_confirmed() {
        let mut unconfirmed = item("a", "Alpha", None);
        unconfirmed.confirmed = false;
        unconfirmed.subscription_count = 1;
        let subs = vec![sub(
            "s1", "a", "Pro", "subscription", Some(1000), Some("USD"), Some("monthly"), Some("2026-12-01"),
        )];
        let report = build_report(&[unconfirmed], &subs, &config(), "now").unwrap();
        assert!(report.recommendations.iter().any(|r| {
            r.recommendation_type == "R5_DATA_GAP"
                && r.missing_information.contains(&"item_confirmation".to_string())
        }));
    }

    #[test]
    fn deterministic_ids_survive_input_ordering() {
        let a = sub("s1", "a", "Plan A", "subscription", Some(2000), Some("USD"), Some("monthly"), None);
        let b = sub("s2", "b", "Plan B", "subscription", Some(3000), Some("USD"), Some("monthly"), None);
        let items_one = vec![item("a", "Alpha", Some("g")), item("b", "Beta", Some("g"))];
        let items_two = vec![item("b", "Beta", Some("g")), item("a", "Alpha", Some("g"))];
        let one = build_report(&items_one, &[a.clone(), b.clone()], &config(), "now").unwrap();
        let two = build_report(&items_two, &[b.clone(), a.clone()], &config(), "now").unwrap();
        let ids_one: Vec<_> = one.recommendations.iter().map(|r| &r.recommendation_id).collect();
        let ids_two: Vec<_> = two.recommendations.iter().map(|r| &r.recommendation_id).collect();
        assert_eq!(ids_one, ids_two, "IDs differ:\none={ids_one:?}\ntwo={ids_two:?}");
    }

    #[test]
    fn full_report_is_byte_stable_for_identical_inputs() {
        let items = vec![
            item("a", "Alpha", Some("g")),
            item("b", "Beta", Some("g")),
            item("c", "Gamma", None),
        ];
        let subs = vec![
            sub("s1", "a", "Plan A", "subscription", Some(2000), Some("USD"), Some("monthly"), Some("2026-11-15")),
            sub("s2", "b", "Plan B", "subscription", Some(6000), Some("USD"), Some("monthly"), None),
            sub("s3", "c", "NoFacts", "subscription", None, None, None, None),
        ];
        let mut cfg = config();
        cfg.cost_thresholds_minor.insert("USD".into(), 5000);
        let one = build_report(&items, &subs, &cfg, "2026-11-12T00:00:00Z").unwrap();
        let two = build_report(&items, &subs, &cfg, "2026-11-12T00:00:00Z").unwrap();
        let a = serde_json::to_string(&one).unwrap();
        let b = serde_json::to_string(&two).unwrap();
        assert_eq!(a, b, "same inputs must serialize byte-identically");
    }

    #[test]
    fn r4_cancel_scenario_produces_single_currency_delta() {
        let items = vec![item("a", "Alpha", None)];
        let subs = vec![
            sub("s1", "a", "Keep", "subscription", Some(1000), Some("USD"), Some("monthly"), None),
            sub("s2", "a", "Drop", "subscription", Some(2000), Some("USD"), Some("monthly"), None),
        ];
        let actions = vec![ScenarioAction::Cancel {
            subscription_id: "s2".to_string(),
        }];
        let result = simulate(&items, &subs, &actions, &config(), "now").unwrap();
        assert_eq!(result.affected_subscriptions, vec!["s2".to_string()]);
        assert_eq!(result.deltas_monthly.len(), 1);
        let delta = &result.deltas_monthly[0];
        assert_eq!(delta.currency, "USD");
        assert_eq!(delta.minor_before, 3000);
        assert_eq!(delta.minor_after, 1000);
        assert_eq!(delta.delta_minor, -2000);
        assert_eq!(result.deltas_annual[0].delta_minor, -24000);
        assert!(result.notes[0].contains("simulation only"));
    }

    #[test]
    fn r4_reduce_and_replace_across_currencies_stay_separate() {
        let items = vec![item("a", "Alpha", None), item("b", "Beta", None)];
        let subs = vec![
            sub("s1", "a", "Usd", "subscription", Some(5000), Some("USD"), Some("monthly"), None),
            sub("s2", "b", "Cny", "subscription", Some(6800), Some("CNY"), Some("monthly"), None),
        ];
        let actions = vec![
            ScenarioAction::Reduce { subscription_id: "s1".to_string(), new_amount_minor: 2000 },
            ScenarioAction::Replace {
                subscription_id: "s2".to_string(),
                new_amount_minor: 3000,
                new_currency: "CNY".to_string(),
                new_period: "yearly".to_string(),
            },
        ];
        let result = simulate(&items, &subs, &actions, &config(), "now").unwrap();
        let monthly: std::collections::HashMap<_, _> = result
            .deltas_monthly
            .iter()
            .map(|d| (d.currency.as_str(), (d.minor_before, d.minor_after, d.delta_minor)))
            .collect();
        assert_eq!(monthly["USD"], (5000, 2000, -3000));
        // Replace moved CNY from 6800/month to 3000/12=250/month.
        assert_eq!(monthly["CNY"], (6800, 250, -6550));
    }

    /// RED (review P1): a `usage` record can carry amount/currency/period, but
    /// its amount is metered, not a foldable fixed fee — the spending overview
    /// and R5 both treat it as a gap. Cancelling it must therefore produce NO
    /// delta, and the notes must name the gap once, consistently.
    #[test]
    fn r4_usage_with_amount_is_never_folded_and_always_reported_as_gap() {
        let items = vec![item("a", "Alpha", None)];
        let subs = vec![
            sub("s1", "a", "Keep", "subscription", Some(1000), Some("USD"), Some("monthly"), None),
            sub("s2", "a", "Metered", "usage", Some(9000), Some("USD"), Some("usage"), None),
        ];
        let result = simulate(
            &items,
            &subs,
            &[ScenarioAction::Cancel { subscription_id: "s2".to_string() }],
            &config(),
            "now",
        )
        .unwrap();
        assert_eq!(result.affected_subscriptions, vec!["s2".to_string()]);
        // No foldable change: the metered record never entered the totals.
        assert!(
            result.deltas_monthly.is_empty(),
            "deltas must stay empty, got {:?}",
            result.deltas_monthly
        );
        assert!(result.deltas_annual.is_empty());
        // The gap explanation must cover the same record, same reason.
        assert!(result.notes.iter().any(|n| n.contains("usage_amount_not_foldable")));
    }

    /// RED (review P1, replace path): replacing into a configuration the
    /// engine cannot fold must not invent savings either.
    /// RED (review round 2, P1): when a Replace fills in the facts a fixed
    /// subscription was missing, the record becomes foldable AFTER the
    /// simulation. The notes must therefore describe exclusion per state —
    /// the record is excluded from the BEFORE totals but counted in the AFTER
    /// totals — instead of a blanket "excluded" claim that contradicts the
    /// arithmetic.

    /// Review round 3: the arithmetic gate and the explanation gate are two
    /// functions with the same arms; this invariant keeps them in lockstep
    /// so a record can never be counted while being reported as a gap.

    /// M2-B review P1-2: the four scenario actions are a frozen JSON contract
    /// shared by the GUI, the CLI and the Tauri layer. Rust's enum-level
    /// `rename_all` does NOT touch variant body fields, so each variant pins
    /// its own casing — this test is the executable form of that contract.
    #[test]
    fn scenario_actions_serialize_with_camel_case_body_fields() {
        let cases: Vec<(ScenarioAction, Value)> = vec![
            (
                ScenarioAction::Cancel {
                    subscription_id: "s1".to_string(),
                },
                serde_json::json!({"kind": "cancel", "subscriptionId": "s1"}),
            ),
            (
                ScenarioAction::Reduce {
                    subscription_id: "s1".to_string(),
                    new_amount_minor: 2000,
                },
                serde_json::json!({
                    "kind": "reduce",
                    "subscriptionId": "s1",
                    "newAmountMinor": 2000
                }),
            ),
            (
                ScenarioAction::Replace {
                    subscription_id: "s1".to_string(),
                    new_amount_minor: 2000,
                    new_currency: "USD".to_string(),
                    new_period: "monthly".to_string(),
                },
                serde_json::json!({
                    "kind": "replace",
                    "subscriptionId": "s1",
                    "newAmountMinor": 2000,
                    "newCurrency": "USD",
                    "newPeriod": "monthly"
                }),
            ),
            (
                ScenarioAction::DeactivateItem {
                    stack_item_id: "a".to_string(),
                },
                serde_json::json!({"kind": "deactivateItem", "stackItemId": "a"}),
            ),
        ];
        for (action, expected) in cases {
            let serialized = serde_json::to_value(&action).expect("serialize action");
            assert_eq!(
                serialized, expected,
                "action wire format drifted: {serialized} != {expected}"
            );
            // Round-trip: the GUI's payload must deserialize back unchanged.
            let round: ScenarioAction =
                serde_json::from_value(expected.clone()).expect("deserialize action");
            assert_eq!(round, action, "round-trip mismatch for {expected}");
        }
    }

    /// Scope note (review round 4): the lockstep invariant covers every
    /// active record EXCEPT the documented usage-without-amount case, where
    /// neither gate applies on purpose — such records are not foldable AND
    /// carry no gap (they surface in the separate `usage_without_bills`
    /// bucket, per spending-overview semantics), so "not foldable" agreeing
    /// with "no gap" is correct, not a drift. Pinned below explicitly.
    #[test]
    fn usage_without_amount_is_neither_foldable_nor_a_gap() {
        let metered = sub("s9", "a", "MeteredNoBills", "usage", None, None, Some("usage"), None);
        assert!(foldable_parts(&metered).is_none());
        assert!(data_gaps(&metered).is_empty());
    }

    #[test]
    fn fold_eligibility_and_gap_explanations_stay_in_lockstep() {
        let cases: Vec<SubscriptionView> = vec![
            sub("s1", "a", "Full", "subscription", Some(1000), Some("USD"), Some("monthly"), None),
            sub("s2", "a", "NoAmount", "subscription", None, Some("USD"), Some("monthly"), None),
            sub("s3", "a", "NoCurrency", "subscription", Some(1000), None, Some("monthly"), None),
            sub("s4", "a", "NoPeriod", "subscription", Some(1000), Some("USD"), None, None),
            sub("s5", "a", "Metered", "usage", Some(9000), Some("USD"), Some("usage"), None),
            sub("s6", "a", "UnknownType", "unknown", None, None, None, None),
            sub("s7", "a", "Free", "free", None, Some("USD"), None, None),
        ];
        let mut paused = sub("s8", "a", "Paused", "subscription", Some(1000), Some("USD"), Some("monthly"), None);
        paused.status = "paused".to_string();
        let cases: Vec<SubscriptionView> = cases.into_iter().chain(std::iter::once(paused)).collect();
        for subview in &cases {
            // Both call sites filter non-active records before invoking these
            // gates, so the lockstep invariant applies to active rows only.
            if subview.status != "active" {
                continue;
            }
            let foldable = foldable_parts(subview).is_some();
            let gapless = data_gaps(subview).is_empty();
            assert_eq!(
                foldable, gapless,
                "eligibility and gap view disagree for {:?} ({}): foldable={foldable} gaps={:?}",
                subview.billing_type, subview.product_name, data_gaps(subview)
            );
        }
    }

    #[test]
    fn r4_replace_filling_missing_facts_moves_the_record_into_the_after_totals() {
        // A fixed subscription whose amount was never registered.
        let incomplete = sub("s1", "a", "Unpriced", "subscription", None, Some("USD"), Some("monthly"), None);
        let items = vec![item("a", "Alpha", None)];
        let result = simulate(
            &items,
            &[incomplete],
            &[ScenarioAction::Replace {
                subscription_id: "s1".to_string(),
                new_amount_minor: 2000,
                new_currency: "USD".to_string(),
                new_period: "monthly".to_string(),
            }],
            &config(),
            "now",
        )
        .unwrap();
        assert_eq!(result.affected_subscriptions, vec!["s1".to_string()]);
        // Arithmetic: excluded before, counted after.
        assert_eq!(result.deltas_monthly.len(), 1);
        assert_eq!(result.deltas_monthly[0].minor_before, 0);
        assert_eq!(result.deltas_monthly[0].minor_after, 2000);
        assert_eq!(result.deltas_monthly[0].delta_minor, 2000);
        // Notes must not claim the record is excluded from the arithmetic it
        // now participates in.
        let before_note = result
            .notes
            .iter()
            .find(|n| n.contains("Excluded from the BEFORE totals"))
            .expect("before-exclusion note required");
        assert!(before_note.contains("s1"), "before note must name s1: {before_note}");
        // No AFTER exclusion may exist at all — and certainly none that
        // still names s1, which now folds into the AFTER totals.
        for note in &result.notes {
            if note.contains("Excluded from the AFTER totals") {
                assert!(
                    !note.contains("s1"),
                    "after-exclusion note must not name s1: {note}"
                );
            }
        }
    }

    #[test]
    fn r4_replace_keeps_eligibility_and_gap_notes_consistent() {
        let items = vec![item("a", "Alpha", None)];
        let subs = vec![
            sub("s1", "a", "Fixed", "subscription", Some(5000), Some("USD"), Some("monthly"), None),
            sub("s2", "a", "Metered", "usage", Some(9000), Some("USD"), Some("usage"), None),
        ];
        // Cancel the foldable one: the delta covers exactly that record. The
        // untouched usage record never entered either total, so the stated
        // saving is real arithmetic, not a metered amount masquerading as one.
        let result = simulate(
            &items,
            &subs,
            &[ScenarioAction::Cancel { subscription_id: "s1".to_string() }],
            &config(),
            "now",
        )
        .unwrap();
        assert_eq!(result.deltas_monthly.len(), 1);
        assert_eq!(result.deltas_monthly[0].minor_before, 5000);
        assert_eq!(result.deltas_monthly[0].minor_after, 0);
        assert_eq!(result.deltas_monthly[0].delta_minor, -5000);
        // And the usage record is explicitly named as excluded.
        assert!(result.notes.iter().any(|n| n.contains("usage_amount_not_foldable")));

        // Reduce a foldable record while a usage record sits untouched: the
        // delta covers only the foldable part, notes still name the gap.
        let result = simulate(
            &items,
            &subs,
            &[ScenarioAction::Reduce {
                subscription_id: "s1".to_string(),
                new_amount_minor: 2000,
            }],
            &config(),
            "now",
        )
        .unwrap();
        assert_eq!(result.deltas_monthly.len(), 1);
        assert_eq!(result.deltas_monthly[0].delta_minor, -3000);
        assert!(result.notes.iter().any(|n| n.contains("usage_amount_not_foldable")));
    }

    /// RED (review P2): the engine must never read the system clock — a
    /// report without a pinned date is an error, so callers decide the date
    /// and results replay deterministically.
    #[test]
    fn report_without_pinned_today_is_rejected() {
        let items = vec![item("a", "Alpha", None)];
        let subs = vec![sub(
            "s1", "a", "Pro", "subscription", Some(1000), Some("USD"), Some("monthly"), None,
        )];
        let err = build_report(&items, &subs, &OptimizerConfig::default(), "now");
        assert!(
            matches!(err, Err(crate::error::Error::InvalidInput(_))),
            "missing today must be an explicit input error, got {:?}",
            err
        );
    }

    #[test]
    fn r4_unknown_ids_are_rejected_as_invalid_input() {
        let items = vec![item("a", "Alpha", None)];
        let subs = vec![sub("s1", "a", "Keep", "subscription", Some(1000), Some("USD"), Some("monthly"), None)];
        let err = simulate(
            &items,
            &subs,
            &[ScenarioAction::Cancel { subscription_id: "nope".to_string() }],
            &config(),
            "now",
        );
        assert!(matches!(err, Err(crate::error::Error::InvalidInput(_))));
    }

    #[test]
    fn r4_deactivate_item_cancels_all_active_subscriptions() {
        let items = vec![item("a", "Alpha", None)];
        let subs = vec![
            sub("s1", "a", "One", "subscription", Some(1000), Some("USD"), Some("monthly"), None),
            sub("s2", "a", "Two", "subscription", Some(2000), Some("CNY"), Some("monthly"), None),
            sub("s3", "a", "Paused", "subscription", Some(3000), Some("USD"), Some("monthly"), None),
        ];
        let mut s3 = subs[2].clone();
        s3.status = "paused".to_string();
        let subs = vec![subs[0].clone(), subs[1].clone(), s3];
        let result = simulate(
            &items,
            &subs,
            &[ScenarioAction::DeactivateItem { stack_item_id: "a".to_string() }],
            &config(),
            "now",
        )
        .unwrap();
        // Only the two active rows are affected; paused stays paused.
        assert_eq!(result.affected_subscriptions.len(), 2);
        assert_eq!(result.deltas_monthly.len(), 2, "USD and CNY stay separate");
    }
}
