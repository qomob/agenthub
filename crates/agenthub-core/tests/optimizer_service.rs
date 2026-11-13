//! Cost Optimizer service-level integration tests (M2-A).
//!
//! The rules engine itself (`optimizer.rs`) is exhaustively unit-tested;
//! these tests prove the read-only service path: `optimizer_report` /
//! `optimizer_simulate` see exactly what the user registered, recommendations
//! degrade per the frozen rules, scenario simulation never writes, and no
//! audit event or database mutation is produced by running the optimizer.
//!
//! Every test runs against an isolated fixture home; nothing touches the
//! real `~/.agenthub` or real agent directories.

mod common;

use agenthub_core::model::*;
use agenthub_core::optimizer::{OptimizerConfig, ScenarioAction};
use common::Fixture;

fn item(name: &str, overlap: Option<&str>) -> StackItemUpsert {
    StackItemUpsert {
        id: None,
        name: name.to_string(),
        vendor: None,
        overlap_group: overlap.map(str::to_string),
        notes: None,
    }
}

fn sub(stack_item_id: &str, amount: Option<i64>, currency: Option<&str>) -> SubscriptionUpsert {
    SubscriptionUpsert {
        id: None,
        stack_item_id: stack_item_id.to_string(),
        product_name: "Test product".to_string(),
        vendor: None,
        plan_name: None,
        billing_type: "subscription".to_string(),
        amount_minor: amount,
        currency: currency.map(str::to_string),
        billing_period: Some("monthly".to_string()),
        started_on: None,
        renews_on: None,
        status: None,
        notes: None,
        info_source: None,
    }
}

#[test]
fn optimizer_report_reads_registered_data_and_r1_silence_without_groups() {
    let f = Fixture::new();
    let hub = f.hub();
    let a = hub.stack_upsert_item(item("Alpha", None)).unwrap();
    let b = hub.stack_upsert_item(item("Beta", None)).unwrap();
    hub.subscription_upsert(sub(&a.id, Some(2000), Some("USD")))
        .unwrap();
    hub.subscription_upsert(sub(&b.id, Some(3000), Some("USD")))
        .unwrap();

    let report = hub.optimizer_report().unwrap();
    assert_eq!(report.counts.stack_items, 2);
    assert_eq!(report.counts.subscriptions_active, 2);
    // R1 stays silent: no overlap_group was ever maintained (M1 fallback).
    assert!(report
        .recommendations
        .iter()
        .all(|r| r.recommendation_type != "R1_OVERLAP"));
}

#[test]
fn optimizer_report_fires_r1_for_shared_group_and_lists_costs() {
    let f = Fixture::new();
    let hub = f.hub();
    let a = hub.stack_upsert_item(item("Alpha", Some("coding"))).unwrap();
    let b = hub.stack_upsert_item(item("Beta", Some("coding"))).unwrap();
    hub.subscription_upsert(sub(&a.id, Some(2000), Some("USD")))
        .unwrap();
    hub.subscription_upsert(sub(&b.id, Some(3000), Some("USD")))
        .unwrap();

    let report = hub.optimizer_report().unwrap();
    let r1 = report
        .recommendations
        .iter()
        .find(|r| r.recommendation_type == "R1_OVERLAP")
        .expect("R1 should fire for a shared overlap group");
    assert_eq!(r1.related_stack_items.len(), 2);
    assert!(r1.explanation.contains("not automatically treated as duplicate charges"));
    // Reproducibility: a second call yields the same deterministic IDs.
    let again = hub.optimizer_report().unwrap();
    let ids = |r: &agenthub_core::optimizer::OptimizerReport| -> Vec<String> {
        r.recommendations.iter().map(|x| x.recommendation_id.clone()).collect()
    };
    assert_eq!(ids(&report), ids(&again));
}

#[test]
fn optimizer_simulate_is_pure_arithmetic_and_never_writes() {
    let f = Fixture::new();
    let hub = f.hub();
    let a = hub.stack_upsert_item(item("Alpha", None)).unwrap();
    let created = hub
        .subscription_upsert(sub(&a.id, Some(2000), Some("USD")))
        .unwrap();

    let result = hub
        .optimizer_simulate(
            vec![ScenarioAction::Cancel {
                subscription_id: created.id.clone(),
            }],
            OptimizerConfig::default(),
        )
        .unwrap();
    let delta = result
        .deltas_monthly
        .iter()
        .find(|d| d.currency == "USD")
        .expect("USD delta expected");
    assert_eq!(delta.minor_before, 2000);
    assert_eq!(delta.minor_after, 0);
    assert_eq!(delta.delta_minor, -2000);
    assert!(result.notes.iter().any(|n| n.contains("simulation only")));

    // The simulation must not have touched the database: the subscription is
    // still active and no audit event was recorded for it.
    let detail = hub.stack_item_detail(&a.id).unwrap();
    assert_eq!(detail.subscriptions.len(), 1);
    assert_eq!(detail.subscriptions[0].status, "active");

    let before = hub.stack_audit_events(1000).unwrap().len();
    hub.optimizer_report().unwrap();
    let after = hub.stack_audit_events(1000).unwrap().len();
    assert_eq!(before, after, "running the optimizer must not write audit events");
}

#[test]
fn optimizer_simulate_rejects_unknown_subscription_as_invalid_input() {
    let f = Fixture::new();
    let hub = f.hub();
    hub.stack_upsert_item(item("Alpha", None)).unwrap();
    let err = hub
        .optimizer_simulate(
            vec![ScenarioAction::Cancel {
                subscription_id: "no-such-sub".to_string(),
            }],
            OptimizerConfig::default(),
        )
        .unwrap_err();
    assert!(matches!(err, agenthub_core::error::Error::InvalidInput(_)));
}

#[test]
fn optimizer_settings_roundtrip_shared_by_gui_and_cli() {
    let f = Fixture::new();
    let hub = f.hub();

    // Default: R2 silent (no thresholds), R3 default window.
    let settings = hub.optimizer_settings().unwrap();
    assert!(settings.cost_thresholds_minor.is_empty());
    assert_eq!(settings.renewal_window_days, None);

    // Persist thresholds and a window; the same read must serve both surfaces.
    let mut thresholds = std::collections::BTreeMap::new();
    thresholds.insert("USD".to_string(), 5000i64);
    let stored = hub
        .update_optimizer_settings(Some(thresholds.clone()), Some(Some(14)))
        .unwrap();
    assert_eq!(stored.cost_thresholds_minor, thresholds);
    assert_eq!(stored.renewal_window_days, Some(14));

    let reread = hub.optimizer_settings().unwrap();
    assert_eq!(reread, stored);

    // The report must now use the persisted threshold (R2 fires).
    hub.stack_upsert_item(item("Alpha", None)).unwrap();
    let items = hub.stack_items().unwrap();
    let a = items.iter().find(|i| i.name == "Alpha").unwrap().id.clone();
    hub.subscription_upsert(sub(&a, Some(6000), Some("USD"))).unwrap();
    let report = hub.optimizer_report().unwrap();
    let r2 = report
        .recommendations
        .iter()
        .find(|r| r.recommendation_type == "R2_HIGH_COST")
        .expect("R2 must fire with the persisted threshold");
    assert!(r2.explanation.contains("60.00 USD"));
}

#[test]
fn optimizer_settings_validation_rejects_bad_parameters() {
    let f = Fixture::new();
    let hub = f.hub();

    // Bad currency code.
    let mut thresholds = std::collections::BTreeMap::new();
    thresholds.insert("usdd".to_string(), 5000i64);
    let err = hub.update_optimizer_settings(Some(thresholds), None).unwrap_err();
    assert!(matches!(err, agenthub_core::error::Error::InvalidInput(_)));

    // Non-positive threshold.
    let mut thresholds = std::collections::BTreeMap::new();
    thresholds.insert("USD".to_string(), 0i64);
    let err = hub.update_optimizer_settings(Some(thresholds), None).unwrap_err();
    assert!(matches!(err, agenthub_core::error::Error::InvalidInput(_)));

    // Window out of range.
    let err = hub
        .update_optimizer_settings(None, Some(Some(31)))
        .unwrap_err();
    assert!(matches!(err, agenthub_core::error::Error::InvalidInput(_)));

    // Window in range passes and clears back to default (None) explicitly.
    hub.update_optimizer_settings(None, Some(None)).unwrap();
    assert_eq!(hub.optimizer_settings().unwrap().renewal_window_days, None);
}
