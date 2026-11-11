//! Spending-overview semantics for My AI Stack (M1-A, FR-3).
//!
//! Frozen rules under test:
//! - USD 20/month + USD 120/year => monthly equivalent 30, annual 360 — and
//!   the monthly equivalent is NOT the actual cash spent this month;
//! - currencies are never summed across codes;
//! - missing amounts never count as zero-cost;
//! - usage billing without bill data is never estimated;
//! - hybrids contribute their known fixed-fee part only;
//! - renewal windows are inclusive on both ends;
//! - G2: every reported equivalent stays within the JSON safe integer range.

mod common;

use agenthub_core::error::Error;
use agenthub_core::model::*;
use common::Fixture;

/// Day offsets for renewal-window tests (YYYY-MM-DD strings).
fn iso_date(offset_days: i64) -> String {
    chrono::Local::now()
        .date_naive()
        .checked_add_signed(chrono::Duration::days(offset_days))
        .unwrap()
        .format("%Y-%m-%d")
        .to_string()
}

fn item(hub: &agenthub_core::AgentHub, name: &str) -> StackItemView {
    hub.stack_upsert_item(StackItemUpsert {
        id: None,
        name: name.to_string(),
        vendor: None,
        overlap_group: None,
        notes: None,
    })
    .unwrap()
}

fn sub(stack_item_id: &str) -> SubscriptionUpsert {
    SubscriptionUpsert {
        id: None,
        stack_item_id: stack_item_id.to_string(),
        product_name: "Product".to_string(),
        vendor: None,
        plan_name: None,
        billing_type: "subscription".to_string(),
        amount_minor: None,
        currency: None,
        billing_period: None,
        started_on: None,
        renews_on: None,
        status: None,
        notes: None,
        info_source: None,
    }
}

#[test]
fn mixed_periods_fold_to_monthly_and_annual_equivalents() {
    let f = Fixture::new();
    let hub = f.hub();
    let a = item(&hub, "Tool A");
    let b = item(&hub, "Tool B");
    // The frozen acceptance sample: A = USD 20/month, B = USD 120/year.
    hub.subscription_upsert(SubscriptionUpsert {
        amount_minor: Some(2_000),
        currency: Some("USD".to_string()),
        billing_period: Some("monthly".to_string()),
        ..sub(&a.id)
    })
    .unwrap();
    hub.subscription_upsert(SubscriptionUpsert {
        amount_minor: Some(12_000),
        currency: Some("USD".to_string()),
        billing_period: Some("yearly".to_string()),
        ..sub(&b.id)
    })
    .unwrap();

    let overview = hub.spending_overview().unwrap();
    assert_eq!(overview.by_currency.len(), 1);
    let usd = &overview.by_currency[0];
    assert_eq!(usd.currency, "USD");
    assert_eq!(usd.monthly_equivalent_minor, 3_000, "monthly equivalent");
    assert_eq!(usd.annual_equivalent_minor, 36_000, "annual equivalent");
    assert_eq!(usd.subscription_count, 2);
    assert_eq!(usd.hybrid_count, 0);
    // The monthly equivalent is a derived average — it must not be presented
    // as this month's cash outflow. The API exposes no such field; the flag
    // documents the invariant that the UI label carries.
    assert!(
        overview.renewals_unknown.len() == 2,
        "no renewal dates registered"
    );
    assert!(overview.incomplete.is_empty());
}

#[test]
fn currencies_are_never_summed_across_codes() {
    let f = Fixture::new();
    let hub = f.hub();
    let a = item(&hub, "USD tool");
    let cny = item(&hub, "CNY tool");
    hub.subscription_upsert(SubscriptionUpsert {
        amount_minor: Some(2_000),
        currency: Some("USD".to_string()),
        billing_period: Some("monthly".to_string()),
        ..sub(&a.id)
    })
    .unwrap();
    hub.subscription_upsert(SubscriptionUpsert {
        amount_minor: Some(15_000),
        currency: Some("CNY".to_string()),
        billing_period: Some("monthly".to_string()),
        ..sub(&cny.id)
    })
    .unwrap();

    let overview = hub.spending_overview().unwrap();
    assert_eq!(overview.by_currency.len(), 2, "separate currency buckets");
    let usd = overview
        .by_currency
        .iter()
        .find(|g| g.currency == "USD")
        .unwrap();
    let cny = overview
        .by_currency
        .iter()
        .find(|g| g.currency == "CNY")
        .unwrap();
    assert_eq!(usd.monthly_equivalent_minor, 2_000);
    assert_eq!(cny.monthly_equivalent_minor, 15_000);
    // No aggregate field exists that could add these codes together.
    assert_eq!(overview.by_currency.len(), 2);
}

#[test]
fn incomplete_facts_are_never_filled_with_zeros() {
    let f = Fixture::new();
    let hub = f.hub();

    // Missing amount: not zero-cost, listed as incomplete["amount"].
    let no_amount = item(&hub, "No amount");
    hub.subscription_upsert(SubscriptionUpsert {
        billing_type: "subscription".to_string(),
        currency: Some("USD".to_string()),
        billing_period: Some("monthly".to_string()),
        amount_minor: None,
        ..sub(&no_amount.id)
    })
    .unwrap();

    // Unknown billing type.
    let unknown_type = item(&hub, "Unknown type");
    hub.subscription_upsert(SubscriptionUpsert {
        billing_type: "unknown".to_string(),
        ..sub(&unknown_type.id)
    })
    .unwrap();

    // Known amount but unknown period: cannot be folded.
    let unknown_period = item(&hub, "Unknown period");
    hub.subscription_upsert(SubscriptionUpsert {
        amount_minor: Some(1_000),
        currency: Some("USD".to_string()),
        billing_period: Some("unknown".to_string()),
        ..sub(&unknown_period.id)
    })
    .unwrap();

    let overview = hub.spending_overview().unwrap();
    assert!(overview.by_currency.is_empty(), "nothing is foldable yet");
    assert_eq!(
        overview.incomplete.len(),
        3,
        "got {:?}",
        overview.incomplete
    );
    let missing_for = |name: &str| -> Vec<String> {
        overview
            .incomplete
            .iter()
            .find(|entry| entry.subscription.stack_item_id == name)
            .map(|entry| entry.missing.clone())
            .unwrap()
    };
    assert_eq!(missing_for(&no_amount.id), vec!["amount".to_string()]);
    assert_eq!(
        missing_for(&unknown_type.id),
        vec!["billing_type".to_string()]
    );
    assert_eq!(
        missing_for(&unknown_period.id),
        vec!["billing_period".to_string()]
    );
    // Counts still track the subscriptions even though nothing folds.
    assert_eq!(overview.counts.total, 3);
    assert_eq!(overview.counts.active, 3);
}

#[test]
fn usage_billing_without_bills_is_never_estimated() {
    let f = Fixture::new();
    let hub = f.hub();
    let usage = item(&hub, "Metered tool");
    hub.subscription_upsert(SubscriptionUpsert {
        billing_type: "usage".to_string(),
        ..sub(&usage.id)
    })
    .unwrap();

    let overview = hub.spending_overview().unwrap();
    assert!(overview.by_currency.is_empty(), "no invented estimates");
    assert_eq!(overview.usage_without_bills.len(), 1);
    assert_eq!(overview.usage_without_bills[0].stack_item_id, usage.id);

    // A metered subscription that carries a recorded amount is still not
    // foldable: the amount is an input figure, not a known bill.
    hub.subscription_upsert(SubscriptionUpsert {
        billing_type: "usage".to_string(),
        amount_minor: Some(500),
        currency: Some("USD".to_string()),
        ..sub(&usage.id)
    })
    .unwrap();
    let overview = hub.spending_overview().unwrap();
    assert!(overview.by_currency.is_empty());
    let entry = overview
        .incomplete
        .iter()
        .find(|e| e.subscription.stack_item_id == usage.id)
        .unwrap();
    assert_eq!(entry.missing, vec!["usage_not_foldable".to_string()]);
}

#[test]
fn hybrids_contribute_known_fixed_fees_only() {
    let f = Fixture::new();
    let hub = f.hub();
    let hybrid = item(&hub, "Hybrid tool");
    hub.subscription_upsert(SubscriptionUpsert {
        billing_type: "hybrid".to_string(),
        amount_minor: Some(2_500),
        currency: Some("USD".to_string()),
        billing_period: Some("monthly".to_string()),
        ..sub(&hybrid.id)
    })
    .unwrap();

    let overview = hub.spending_overview().unwrap();
    let usd = &overview.by_currency[0];
    assert_eq!(usd.monthly_equivalent_minor, 2_500);
    assert_eq!(
        usd.hybrid_count, 1,
        "UI must label hybrid totals as fixed-fee only"
    );
}

#[test]
fn free_subscriptions_are_known_zero_cost_not_incomplete() {
    let f = Fixture::new();
    let hub = f.hub();
    let free = item(&hub, "Free tool");
    hub.subscription_upsert(SubscriptionUpsert {
        billing_type: "free".to_string(),
        ..sub(&free.id)
    })
    .unwrap();

    let overview = hub.spending_overview().unwrap();
    assert!(overview.by_currency.is_empty());
    assert!(
        overview.incomplete.is_empty(),
        "free is complete: zero-cost is known"
    );
    assert!(overview.usage_without_bills.is_empty());
}

#[test]
fn renewal_windows_are_inclusive_and_classified() {
    let f = Fixture::new();
    let hub = f.hub();
    let today = item(&hub, "Renews today");
    let inside = item(&hub, "Renews in 6 days");
    let boundary = item(&hub, "Renews in 7 days (window end)");
    let outside = item(&hub, "Renews in 8 days");
    let past = item(&hub, "Renewed last week");
    let never = item(&hub, "No renewal date");

    for (target, offset) in [
        (&today.id, 0i64),
        (&inside.id, 6),
        (&boundary.id, 7),
        (&outside.id, 8),
        (&past.id, -7),
    ] {
        hub.subscription_upsert(SubscriptionUpsert {
            billing_type: "subscription".to_string(),
            amount_minor: Some(1_000),
            currency: Some("USD".to_string()),
            billing_period: Some("monthly".to_string()),
            renews_on: Some(iso_date(offset)),
            ..sub(target)
        })
        .unwrap();
    }
    hub.subscription_upsert(SubscriptionUpsert {
        billing_type: "subscription".to_string(),
        amount_minor: Some(1_000),
        currency: Some("USD".to_string()),
        billing_period: Some("monthly".to_string()),
        ..sub(&never.id)
    })
    .unwrap();

    let overview = hub.spending_overview_with_window(7).unwrap();
    assert_eq!(overview.upcoming_window_days, 7);
    let ids = |list: &[SubscriptionView]| -> Vec<String> {
        list.iter().map(|s| s.stack_item_id.clone()).collect()
    };
    let upcoming = ids(&overview.upcoming_renewals);
    assert!(upcoming.contains(&today.id), "today is inside the window");
    assert!(upcoming.contains(&inside.id));
    assert!(upcoming.contains(&boundary.id), "window end is inclusive");
    assert_eq!(upcoming.len(), 3);
    assert!(ids(&overview.later_renewals).contains(&outside.id));
    assert!(ids(&overview.stale_renewals).contains(&past.id));
    assert!(ids(&overview.renewals_unknown).contains(&never.id));
}

#[test]
fn cancelled_and_paused_subscriptions_leave_the_fold_but_not_the_counts() {
    let f = Fixture::new();
    let hub = f.hub();
    let a = item(&hub, "A");
    let created = hub
        .subscription_upsert(SubscriptionUpsert {
            amount_minor: Some(2_000),
            currency: Some("USD".to_string()),
            billing_period: Some("monthly".to_string()),
            ..sub(&a.id)
        })
        .unwrap();

    let overview = hub.spending_overview().unwrap();
    assert_eq!(overview.by_currency[0].monthly_equivalent_minor, 2_000);

    hub.subscription_upsert(SubscriptionUpsert {
        id: Some(created.id.clone()),
        status: Some("cancelled".to_string()),
        ..SubscriptionUpsert {
            amount_minor: Some(2_000),
            currency: Some("USD".to_string()),
            billing_period: Some("monthly".to_string()),
            ..sub(&a.id)
        }
    })
    .unwrap();

    let overview = hub.spending_overview().unwrap();
    assert!(
        overview.by_currency.is_empty(),
        "cancelled subs do not fold"
    );
    assert_eq!(overview.counts.total, 1, "records remain counted");
    assert_eq!(overview.counts.cancelled, 1);
    assert_eq!(overview.counts.active, 0);
    assert!(
        overview.incomplete.is_empty(),
        "cancellation is not an information gap"
    );
}

#[test]
fn jpy_and_zero_exponent_currencies_are_accepted() {
    let f = Fixture::new();
    let hub = f.hub();
    let jpy = item(&hub, "JPY tool");
    hub.subscription_upsert(SubscriptionUpsert {
        amount_minor: Some(1_000),
        currency: Some("JPY".to_string()),
        billing_period: Some("monthly".to_string()),
        ..sub(&jpy.id)
    })
    .unwrap();
    let overview = hub.spending_overview().unwrap();
    assert_eq!(overview.by_currency[0].currency, "JPY");
    assert_eq!(overview.by_currency[0].monthly_equivalent_minor, 1_000);
    // ISO-4217 exponent (display layer): JPY carries 0 decimal places.
    assert_eq!(agenthub_core::money::currency_exponent("JPY"), Some(0));
    assert_eq!(agenthub_core::money::currency_exponent("USD"), Some(2));
    assert_eq!(agenthub_core::money::currency_exponent("KWD"), Some(3));
}

#[test]
fn g2_output_guard_blocks_unrepresentable_totals() {
    let f = Fixture::new();
    let hub = f.hub();
    // Two individually valid yearly subscriptions whose exact annual sum
    // exceeds the JSON safe integer range: the aggregate must return a
    // structured error instead of a silently rounded number.
    let a = item(&hub, "Big A");
    let b = item(&hub, "Big B");
    for target in [&a.id, &b.id] {
        hub.subscription_upsert(SubscriptionUpsert {
            amount_minor: Some(4_600_000_000_000_000),
            currency: Some("USD".to_string()),
            billing_period: Some("yearly".to_string()),
            ..sub(target)
        })
        .unwrap();
    }
    let err = hub.spending_overview().unwrap_err();
    assert!(matches!(err, Error::InvalidInput(_)), "got {err:?}");
}

#[test]
fn rounding_order_keeps_half_up_semantics() {
    let f = Fixture::new();
    let hub = f.hub();
    // Two CNY 20.00 semiannual subscriptions: exact annual = 8000 minor
    // (2000 × 12/6 each), monthly equivalent = 8000/12 = 666.67 → 667, and
    // never 333 + 333 = 666 (per-line rounding would distort the total).
    let a = item(&hub, "Semi A");
    let b = item(&hub, "Semi B");
    for target in [&a.id, &b.id] {
        hub.subscription_upsert(SubscriptionUpsert {
            amount_minor: Some(2_000),
            currency: Some("CNY".to_string()),
            billing_period: Some("semiannual".to_string()),
            ..sub(target)
        })
        .unwrap();
    }
    let overview = hub.spending_overview().unwrap();
    let cny = &overview.by_currency[0];
    assert_eq!(cny.annual_equivalent_minor, 8_000);
    assert_eq!(cny.monthly_equivalent_minor, 667, "one final rounding step");
}
