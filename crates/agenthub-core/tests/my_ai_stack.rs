//! My AI Stack core-service integration tests (M1-A).
//!
//! Covers: detection refresh attach/idempotency rules, stack-item CRUD,
//! confirmation, G1 merge invariants (root-only, depth-capped, reversible,
//! cost-preserving), delete constraints (P0-4), subscription CRUD with the
//! D-2 three-state error contract, and audit-event recording.
//!
//! Every test runs against an isolated fixture home; nothing touches the real
//! `~/.agenthub` or real agent directories.

mod common;

use agenthub_core::contract::{ApiError, ErrorCode};
use agenthub_core::error::Error;
use agenthub_core::model::*;
use common::Fixture;

fn item(name: &str) -> StackItemUpsert {
    StackItemUpsert {
        id: None,
        name: name.to_string(),
        vendor: None,
        overlap_group: None,
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

// -- detection refresh ------------------------------------------------------

#[test]
fn refresh_creates_items_and_is_idempotent() {
    let f = Fixture::new(); // .codex and .workbuddy both detected installed
    let hub = f.hub();
    // A fresh fixture home: every built-in adapter produces a detection
    // (installed or not), so the first refresh creates one item per adapter
    // (rule ③) and attaches none to pre-existing items.
    let first = hub.stack_refresh_detections().unwrap();
    assert_eq!(first.detections_total, 11);
    assert_eq!(first.items_created, 11);
    assert_eq!(first.items_attached, 0);

    let items = hub.stack_items().unwrap();
    assert_eq!(items.len(), 11);
    for view in &items {
        assert_eq!(view.source, "detected");
        assert!(!view.confirmed);
        assert_eq!(view.detection_count, 1);
        assert_eq!(view.subscription_state, "none");
    }
    assert_eq!(
        items
            .iter()
            .filter(|v| v.detection_state == "installed")
            .count()
            + items
                .iter()
                .filter(|v| v.detection_state == "not_installed")
                .count(),
        11,
        "A/B/C states partition all first-refresh items"
    );
    // At least the two fixture agents (.codex, .workbuddy) are installed.
    let codex = hub
        .stack_items()
        .unwrap()
        .iter()
        .find(|v| {
            hub.stack_item_detail(&v.id)
                .unwrap()
                .detections
                .iter()
                .any(|d| d.adapter_key == "codex")
        })
        .expect("codex item must exist")
        .clone();
    assert_eq!(codex.detection_state, "installed");

    let second = hub.stack_refresh_detections().unwrap();
    assert_eq!(
        second.items_created, 0,
        "3.8b: refresh must not recreate items"
    );
    assert_eq!(second.items_attached, 11);
    let items_after = hub.stack_items().unwrap();
    assert_eq!(items_after.len(), 11);
    assert_eq!(
        items_after.iter().map(|v| v.id.clone()).collect::<Vec<_>>(),
        items.iter().map(|v| v.id.clone()).collect::<Vec<_>>(),
    );
    // Detection row ids are stable across refreshes (I2a UPSERT).
    for view in &items_after {
        let detail = hub.stack_item_detail(&view.id).unwrap();
        assert_eq!(detail.detections.len(), 1);
    }
}

#[test]
fn refresh_keeps_merged_and_confirmed_items_without_duplicates() {
    // 3.8: merged children are never re-created; confirmed items are never
    // re-created; their detection facts refresh in place.
    let f = Fixture::with_agents(&[".trae", ".trae-cn"]);
    let hub = f.hub();
    hub.stack_refresh_detections().unwrap();
    let item_for = |key: &str| -> String {
        hub.stack_items()
            .unwrap()
            .iter()
            .find(|v| {
                hub.stack_item_detail(&v.id)
                    .unwrap()
                    .detections
                    .iter()
                    .any(|d| d.adapter_key == key)
            })
            .unwrap_or_else(|| panic!("no item for adapter {key}"))
            .id
            .clone()
    };
    let b = item_for("trae");
    let a = item_for("trae-cn");

    // Merge trae into trae-cn, then refresh.
    hub.stack_merge_items(&b, &a).unwrap();
    hub.stack_refresh_detections().unwrap();

    let items = hub.stack_items().unwrap();
    assert_eq!(
        items.len(),
        11,
        "3.8: refresh must not create duplicates after merge"
    );
    let merged = items.iter().find(|v| v.id == b).unwrap();
    assert_eq!(merged.merged_into.as_deref(), Some(a.as_str()));
    assert_eq!(
        merged.detection_count, 1,
        "merged child keeps its detection facts"
    );

    // Confirm the target, refresh again: still no duplicates.
    hub.stack_confirm_item(&a).unwrap();
    hub.stack_refresh_detections().unwrap();
    let items = hub.stack_items().unwrap();
    assert_eq!(items.len(), 11);
    assert!(items.iter().find(|v| v.id == a).unwrap().confirmed);
}

#[test]
fn detection_state_follows_rules_a_b_c() {
    // Rule B: only not_installed detections => not_installed.
    let f = Fixture::new();
    let hub = f.hub();
    hub.stack_refresh_detections().unwrap();

    // Rule C: a manual item with no detections is unknown.
    let manual = hub.stack_upsert_item(item("Custom tool")).unwrap();
    assert_eq!(manual.source, "manual");
    assert_eq!(manual.detection_state, "unknown");
    assert_eq!(manual.detection_count, 0);
}

// -- stack item CRUD --------------------------------------------------------

#[test]
fn stack_upsert_create_edit_and_not_found() {
    let f = Fixture::new();
    let hub = f.hub();
    let created = hub.stack_upsert_item(item("My note tool")).unwrap();
    assert!(!created.id.is_empty());
    assert_eq!(created.name, "My note tool");
    assert_eq!(created.source, "manual");

    let edited = hub
        .stack_upsert_item(StackItemUpsert {
            id: Some(created.id.clone()),
            name: "My renamed tool".to_string(),
            vendor: Some("Example".to_string()),
            overlap_group: Some("code-assistant".to_string()),
            notes: Some("kept for review".to_string()),
        })
        .unwrap();
    assert_eq!(edited.name, "My renamed tool");
    assert_eq!(edited.vendor.as_deref(), Some("Example"));
    assert_eq!(
        edited.created_at, created.created_at,
        "created_at must not move on edit"
    );

    let err = hub
        .stack_upsert_item(StackItemUpsert {
            id: Some("st-missing".to_string()),
            name: "x".to_string(),
            vendor: None,
            overlap_group: None,
            notes: None,
        })
        .unwrap_err();
    assert!(
        matches!(err, Error::NotFound(_)),
        "edit of missing id is NotFound, got {err:?}"
    );

    let err = hub
        .stack_upsert_item(StackItemUpsert {
            id: None,
            name: "   ".to_string(),
            vendor: None,
            overlap_group: None,
            notes: None,
        })
        .unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "blank name is invalid input, got {err:?}"
    );
}

#[test]
fn confirm_is_idempotent_and_stamps_last_confirmed_at() {
    let f = Fixture::new();
    let hub = f.hub();
    let view = hub.stack_upsert_item(item("Confirmed tool")).unwrap();
    assert!(view.last_confirmed_at.is_none());
    let confirmed = hub.stack_confirm_item(&view.id).unwrap();
    assert!(confirmed.confirmed);
    assert!(confirmed.last_confirmed_at.is_some());
    let again = hub.stack_confirm_item(&view.id).unwrap();
    assert_eq!(again.last_confirmed_at, confirmed.last_confirmed_at);
}

#[test]
fn delete_is_blocked_while_subscriptions_or_children_exist() {
    let f = Fixture::new();
    let hub = f.hub();
    let parent = hub.stack_upsert_item(item("Parent")).unwrap();
    let child = hub.stack_upsert_item(item("Child")).unwrap();
    hub.stack_merge_items(&child.id, &parent.id).unwrap();

    // Deleting a parent with merged children is a resource conflict.
    let err = hub.stack_delete_item(&parent.id).unwrap_err();
    assert!(matches!(err, Error::ResourceConflict(_)), "got {err:?}");

    // Unmerge, then attach a subscription: deleting stays blocked (P0-4,
    // any subscription status).
    hub.stack_unmerge_item(&child.id).unwrap();
    let s = hub
        .subscription_upsert(sub(&parent.id, Some(2_000), Some("USD")))
        .unwrap();
    for status in ["active", "paused", "cancelled", "expired"] {
        hub.subscription_upsert(SubscriptionUpsert {
            id: Some(s.id.clone()),
            stack_item_id: parent.id.clone(),
            product_name: "Test product".to_string(),
            billing_type: "subscription".to_string(),
            status: Some(status.to_string()),
            ..sub(&parent.id, Some(2_000), Some("USD"))
        })
        .unwrap();
        let err = hub.stack_delete_item(&parent.id).unwrap_err();
        assert!(
            matches!(err, Error::ResourceConflict(_)),
            "deleting with a {status} subscription must conflict, got {err:?}"
        );
    }

    hub.subscription_delete(&s.id).unwrap();
    hub.stack_delete_item(&parent.id).unwrap();
    assert!(hub.stack_items().unwrap().iter().all(|v| v.id != parent.id));
}

// -- merge (G1) -------------------------------------------------------------

#[test]
fn merge_requires_root_items_and_is_reversible() {
    let f = Fixture::new();
    let hub = f.hub();
    let a = hub.stack_upsert_item(item("A")).unwrap();
    let b = hub.stack_upsert_item(item("B")).unwrap();
    let c = hub.stack_upsert_item(item("C")).unwrap();

    // Self-merge refused.
    let err = hub.stack_merge_items(&a.id, &a.id).unwrap_err();
    assert!(matches!(err, Error::ResourceConflict(_)));

    hub.stack_merge_items(&b.id, &a.id).unwrap();
    // G1: a merged child can never be a source again...
    let err = hub.stack_merge_items(&b.id, &c.id).unwrap_err();
    assert!(matches!(err, Error::ResourceConflict(_)), "got {err:?}");
    // ...and an item with merged children can never be a source (A -> C is
    // the regression case from the authorization review: A and C are both
    // roots, but A already holds B).
    let err = hub.stack_merge_items(&a.id, &c.id).unwrap_err();
    assert!(matches!(err, Error::ResourceConflict(_)), "got {err:?}");
    // A remains a legal TARGET (it is still a root).
    hub.stack_merge_items(&c.id, &a.id).unwrap();

    // Unmerge restores the root and keeps subscription FKs untouched.
    hub.stack_unmerge_item(&b.id).unwrap();
    let s = hub
        .subscription_upsert(sub(&b.id, Some(1_000), Some("USD")))
        .unwrap();
    let detail = hub.stack_item_detail(&b.id).unwrap();
    assert!(detail.item.merged_into.is_none());
    assert_eq!(detail.subscriptions.len(), 1);
    assert_eq!(detail.subscriptions[0].id, s.id);
    assert_eq!(detail.subscriptions[0].stack_item_id, b.id);
}

#[test]
fn merge_depth_is_capped_at_one_level() {
    let f = Fixture::new();
    let hub = f.hub();
    let a = hub.stack_upsert_item(item("A")).unwrap();
    let b = hub.stack_upsert_item(item("B")).unwrap();
    let c = hub.stack_upsert_item(item("C")).unwrap();
    hub.stack_merge_items(&b.id, &a.id).unwrap();
    // C -> B would build a chain (B has no children itself, but B is merged).
    let err = hub.stack_merge_items(&c.id, &b.id).unwrap_err();
    assert!(matches!(err, Error::ResourceConflict(_)), "got {err:?}");
    // B already has no children but A has one: merging A into C is refused
    // because A has merged children (G1 source must be a leaf root).
    let err = hub.stack_merge_items(&a.id, &c.id).unwrap_err();
    assert!(matches!(err, Error::ResourceConflict(_)));
}

#[test]
fn merge_group_cost_totals_are_preserved() {
    let f = Fixture::new();
    let hub = f.hub();
    let a = hub.stack_upsert_item(item("A")).unwrap();
    let b = hub.stack_upsert_item(item("B")).unwrap();
    hub.subscription_upsert(sub(&a.id, Some(2_000), Some("USD")))
        .unwrap();
    hub.subscription_upsert(sub(&b.id, Some(1_000), Some("USD")))
        .unwrap();

    let before = hub.spending_overview().unwrap();
    hub.stack_merge_items(&b.id, &a.id).unwrap();
    let after = hub.spending_overview().unwrap();
    assert_eq!(
        before.by_currency[0].annual_equivalent_minor, after.by_currency[0].annual_equivalent_minor,
        "P0-2: merge must not move money between groups"
    );
    assert_eq!(after.by_currency[0].subscription_count, 2);
    // The merged child is still reachable through the group view.
    let detail = hub.stack_item_detail(&a.id).unwrap();
    assert_eq!(detail.merged_children.len(), 1);
}

// -- subscription CRUD ------------------------------------------------------

#[test]
fn subscription_validation_and_three_state_errors() {
    let f = Fixture::new();
    let hub = f.hub();

    // Unknown stack item => NotFound.
    let err = hub
        .subscription_upsert(sub("st-missing", Some(1_000), Some("USD")))
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "got {err:?}");

    let item = hub.stack_upsert_item(item("Tools")).unwrap();

    // Amount without currency => invalid input.
    let err = hub
        .subscription_upsert(sub(&item.id, Some(1_000), None))
        .unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "got {err:?}");

    // Unknown billing_type / status / period => invalid input.
    for mutate in [
        |s: &mut SubscriptionUpsert| s.billing_type = "prepaid".to_string(),
        |s: &mut SubscriptionUpsert| s.status = Some("frozen".to_string()),
        |s: &mut SubscriptionUpsert| s.billing_period = Some("weekly".to_string()),
    ] {
        let mut bad = sub(&item.id, Some(1_000), Some("USD"));
        mutate(&mut bad);
        let err = hub.subscription_upsert(bad).unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "got {err:?}");
    }

    // Bad dates => invalid input.
    let mut bad = sub(&item.id, Some(1_000), Some("USD"));
    bad.renews_on = Some("11/30/2026".to_string());
    let err = hub.subscription_upsert(bad).unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "got {err:?}");

    // 2^53 exceeds the JSON safe range => invalid input (3.9).
    let mut bad = sub(&item.id, Some(9_007_199_254_740_992), Some("USD"));
    bad.billing_period = Some("yearly".to_string());
    let err = hub.subscription_upsert(bad).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "2^53 must be rejected, got {err:?}"
    );

    // 2^53-1 is accepted; with a monthly period its annual equivalent would
    // overflow, so a monthly period must be refused (G2 output guard).
    let mut ok = sub(&item.id, Some(9_007_199_254_740_991), Some("USD"));
    ok.billing_period = Some("yearly".to_string());
    hub.subscription_upsert(ok).unwrap();
    let mut overflow = sub(&item.id, Some(9_007_199_254_740_991), Some("USD"));
    overflow.billing_period = Some("monthly".to_string());
    let err = hub.subscription_upsert(overflow).unwrap_err();
    assert!(
        matches!(err, Error::InvalidInput(_)),
        "annual fold of MAX_SAFE monthly overflows, got {err:?}"
    );

    // Updating a missing subscription => NotFound.
    let err = hub
        .subscription_upsert(SubscriptionUpsert {
            id: Some("sub-missing".to_string()),
            ..sub(&item.id, Some(1_000), Some("USD"))
        })
        .unwrap_err();
    assert!(matches!(err, Error::NotFound(_)), "got {err:?}");

    // created_at survives updates; last_confirmed_at advances.
    let created = hub
        .subscription_upsert(sub(&item.id, Some(2_000), Some("USD")))
        .unwrap();
    let updated = hub
        .subscription_upsert(SubscriptionUpsert {
            id: Some(created.id.clone()),
            ..sub(&item.id, Some(3_000), Some("USD"))
        })
        .unwrap();
    assert_eq!(updated.created_at, created.created_at);
    assert_eq!(updated.amount_minor, Some(3_000));
    assert!(updated.last_confirmed_at.is_some());
}

#[test]
fn subscriptions_on_merged_children_are_refused() {
    let f = Fixture::new();
    let hub = f.hub();
    let a = hub.stack_upsert_item(item("A")).unwrap();
    let b = hub.stack_upsert_item(item("B")).unwrap();
    hub.stack_merge_items(&b.id, &a.id).unwrap();
    let err = hub
        .subscription_upsert(sub(&b.id, Some(1_000), Some("USD")))
        .unwrap_err();
    assert!(matches!(err, Error::ResourceConflict(_)), "got {err:?}");
}

#[test]
fn audit_events_record_stack_changes() {
    let f = Fixture::new();
    let hub = f.hub();
    hub.stack_refresh_detections().unwrap();
    let view = hub.stack_upsert_item(item("Audited")).unwrap();
    hub.stack_confirm_item(&view.id).unwrap();
    let s = hub
        .subscription_upsert(sub(&view.id, Some(1_500), Some("CNY")))
        .unwrap();
    hub.subscription_delete(&s.id).unwrap();
    hub.stack_delete_item(&view.id).unwrap();

    let audits = hub.stack_audit_events(100).unwrap();
    let keys: Vec<&str> = audits.iter().map(|a| a.event_key.as_str()).collect();
    for expected in [
        "stack.refresh",
        "stack.item.upsert",
        "stack.item.confirm",
        "subscription.upsert",
        "subscription.delete",
        "stack.item.delete",
    ] {
        assert!(
            keys.contains(&expected),
            "audit_events must contain {expected}; got {keys:?}"
        );
    }
}

#[test]
fn error_contract_maps_stack_errors() {
    let f = Fixture::new();
    let hub = f.hub();
    // INVALID_INPUT for parameter-level failures.
    let err = hub
        .stack_upsert_item(StackItemUpsert {
            id: None,
            name: "".to_string(),
            vendor: None,
            overlap_group: None,
            notes: None,
        })
        .unwrap_err();
    let envelope = ApiError::from_error(&err);
    assert_eq!(envelope.code, ErrorCode::InvalidInput);
    assert_eq!(envelope.retryable, false);
    // RESOURCE_CONFLICT exit 4 for state conflicts (same arm as PlanStale).
    assert_eq!(
        ErrorCode::ResourceConflict.exit_code(),
        ErrorCode::PlanStale.exit_code()
    );
    assert_eq!(ErrorCode::ResourceConflict.as_str(), "RESOURCE_CONFLICT");
}

#[test]
fn double_submit_upsert_is_idempotent() {
    // 11: repeated submissions must not duplicate records. The service layer
    // treats one subscription per stack item + product identity, so re-submit
    // with the returned id and verify nothing was created.
    let f = Fixture::new();
    let hub = f.hub();
    let target = hub.stack_upsert_item(item("Dup check")).unwrap();
    let payload = |id: Option<String>, item_id: &str| SubscriptionUpsert {
        id,
        stack_item_id: item_id.to_string(),
        product_name: "Cursor Pro".to_string(),
        vendor: Some("Anysphere".to_string()),
        plan_name: Some("Pro".to_string()),
        billing_type: "subscription".to_string(),
        amount_minor: Some(2_000),
        currency: Some("USD".to_string()),
        billing_period: Some("monthly".to_string()),
        started_on: None,
        renews_on: None,
        status: None,
        notes: None,
        info_source: None,
    };
    let first = hub.subscription_upsert(payload(None, &target.id)).unwrap();
    let second = hub
        .subscription_upsert(payload(Some(first.id.clone()), &target.id))
        .unwrap();
    assert_eq!(
        second.id, first.id,
        "double submit updates, never duplicates"
    );
    assert_eq!(hub.subscriptions().unwrap().len(), 1);
    assert_eq!(
        second.created_at, first.created_at,
        "created_at survives updates"
    );
}

#[test]
fn stack_writes_hold_the_runtime_lock() {
    // The runtime_guard counter must be released after each write (a leaked
    // lease would fail every subsequent write with flock busy).
    let f = Fixture::new();
    let hub = f.hub();
    let created = hub.stack_upsert_item(item("Lock check")).unwrap();
    hub.stack_confirm_item(&created.id).unwrap();
    let merged = hub.stack_upsert_item(item("Merge source")).unwrap();
    hub.stack_merge_items(&merged.id, &created.id).unwrap();
    // A failing write must also release the lease afterwards.
    hub.stack_delete_item(&created.id).unwrap_err();
    hub.stack_unmerge_item(&merged.id).unwrap();
    let s = hub
        .subscription_upsert(sub(&created.id, Some(1_000), Some("USD")))
        .unwrap();
    hub.subscription_delete(&s.id).unwrap();
    hub.stack_delete_item(&created.id).unwrap();
    hub.stack_delete_item(&merged.id).unwrap();
    // All writes above succeeded in sequence — no leaked runtime lease.
    assert!(hub
        .stack_items()
        .unwrap()
        .iter()
        .all(|v| v.id == created.id || v.merged_into.is_none()));
}
