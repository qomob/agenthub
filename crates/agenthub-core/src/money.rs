//! Monetary arithmetic for My AI Stack subscriptions (M1-A).
//!
//! Design rules frozen in `docs/proposals/DATA_MODEL_PROPOSAL.md` §4 and
//! gate G2 of the M1-A authorization:
//!
//! - Money is stored as integer minor units (`amount_minor`) with an explicit
//!   ISO-4217 currency code; no floating point ever enters a monetary path.
//! - The JSON contract accepts only minor values within the JavaScript safe
//!   integer range `[-(2^53-1), 2^53-1]` so no precision is lost in transit.
//! - Internal computation uses `i128` with checked arithmetic. Every value
//!   that leaves the module (annual equivalent, monthly equivalent, per-currency
//!   aggregate totals) is re-checked against the safe range; overflow returns a
//!   structured [`Error::InvalidInput`] instead of truncating, wrapping or
//!   panicking.
//! - `annual_equivalent = amount_minor × 12 / period_months` is always exact
//!   because 12 is divisible by every foldable period length {1, 3, 6, 12}.
//!   It is computed from the raw amount, never reconstructed from a rounded
//!   monthly figure.
//! - `monthly_equivalent = round_half_up(amount_minor / period_months)`.
//! - Per-currency aggregation sums the exact annual figures first and derives
//!   the monthly equivalent from the total with a single rounding step.
//!   Rounding per-subscription first would distort the total (two CNY 1000
//!   semiannual subscriptions round to 167 + 167 = 334 per item but the
//!   accurate group figure is 2000/6 = 333).

use crate::error::{Error, Result};

/// Largest integer that is exactly representable as a JavaScript `Number`
/// (`Number.MAX_SAFE_INTEGER`). The API contract rejects minor amounts and
/// output figures beyond this bound.
pub const MAX_SAFE_INTEGER: i64 = 9_007_199_254_740_991;

/// Default look-ahead window for "upcoming renewals", in days, until a
/// settings field is introduced (M2-A). Mirrors the M0 frozen design.
pub const DEFAULT_UPCOMING_WINDOW_DAYS: i64 = 7;

/// ISO-4217 minor-unit exponents for currencies a user may realistically
/// register. Currencies outside this table are rejected with a validation
/// error so exponent ambiguity can never corrupt a stored amount.
pub fn currency_exponent(code: &str) -> Option<u8> {
    match code {
        "USD" | "EUR" | "CNY" | "GBP" | "CAD" | "AUD" | "SGD" | "HKD" | "TWD" | "KRW" | "INR"
        | "CHF" | "SEK" | "NOK" | "DKK" | "NZD" | "BRL" | "MXN" | "RUB" => Some(2),
        // Zero-decimal currencies: amounts are stored in whole units.
        "JPY" => Some(0),
        // Three-decimal currencies.
        "KWD" | "BHD" | "OMR" | "JOD" | "TND" => Some(3),
        _ => None,
    }
}

/// Subscriptions may only be folded into cost figures when their billing
/// period is a known fixed length. `usage` (metered) and `unknown` periods
/// must stay out of foldable aggregates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BillingPeriod {
    Monthly,
    Quarterly,
    Semiannual,
    Yearly,
}

impl BillingPeriod {
    /// Serde/storage representation (snake_case, frozen in M0).
    pub fn as_str(self) -> &'static str {
        match self {
            BillingPeriod::Monthly => "monthly",
            BillingPeriod::Quarterly => "quarterly",
            BillingPeriod::Semiannual => "semiannual",
            BillingPeriod::Yearly => "yearly",
        }
    }

    /// Length of the period in months (the fold divisor).
    pub fn months(self) -> i64 {
        match self {
            BillingPeriod::Monthly => 1,
            BillingPeriod::Quarterly => 3,
            BillingPeriod::Semiannual => 6,
            BillingPeriod::Yearly => 12,
        }
    }

    pub fn parse(value: &str) -> Option<Self> {
        match value {
            "monthly" => Some(BillingPeriod::Monthly),
            "quarterly" => Some(BillingPeriod::Quarterly),
            "semiannual" => Some(BillingPeriod::Semiannual),
            "yearly" => Some(BillingPeriod::Yearly),
            _ => None,
        }
    }
}

/// Validates a user-supplied currency code: three uppercase letters known to
/// the static ISO-4217 exponent table.
pub fn validate_currency(code: &str) -> Result<()> {
    if code.len() != 3 || !code.chars().all(|c| c.is_ascii_uppercase()) {
        return Err(Error::invalid_input(format!(
            "currency must be a 3-letter ISO-4217 code, got {:?}",
            code
        )));
    }
    if currency_exponent(code).is_none() {
        return Err(Error::invalid_input(format!(
            "unsupported currency {:?}: not in the supported ISO-4217 table",
            code
        )));
    }
    Ok(())
}

/// Validates a user-supplied minor amount against the JSON contract bound
/// (2^53-1) and the domain bound (positive, non-zero).
pub fn validate_amount_minor(minor: i64) -> Result<()> {
    if minor <= 0 {
        return Err(Error::invalid_input(
            "amount_minor must be a positive integer; missing amounts stay null instead of zero",
        ));
    }
    if minor > MAX_SAFE_INTEGER {
        return Err(Error::invalid_input(format!(
            "amount_minor {} exceeds the JSON safe integer bound {}",
            minor, MAX_SAFE_INTEGER
        )));
    }
    Ok(())
}

/// Defensive check applied to every value leaving the money module: the figure
/// must be representable in the JSON contract range.
fn checked_output(wide: i128, what: &str) -> Result<i64> {
    if wide < 0 {
        return Err(Error::invalid_input(format!(
            "{} is negative after folding; monetary values must stay non-negative",
            what
        )));
    }
    if wide > MAX_SAFE_INTEGER as i128 {
        return Err(Error::invalid_input(format!(
            "{} {} exceeds the JSON safe integer bound {}; the figure would lose precision in JSON",
            what, wide, MAX_SAFE_INTEGER
        )));
    }
    Ok(wide as i64)
}

/// Rounds half up for non-negative integer division (`numerator / denominator`).
/// Exact rational behaviour without floats.
fn div_round_half_up(numerator: i128, denominator: i64) -> i128 {
    debug_assert!(denominator > 0);
    let d = denominator as i128;
    let quotient = numerator / d;
    let remainder = numerator % d;
    if remainder * 2 >= d {
        quotient + 1
    } else {
        quotient
    }
}

/// Exact annual equivalent: `amount_minor × 12 / period_months`. Because 12 is
/// divisible by every foldable period length, the result is always an exact
/// integer for in-range inputs. Returns a structured error when the exact
/// figure leaves the JSON-safe range (G2: a legal input may still overflow on
/// output, e.g. 9,007,199,254,740,991 monthly × 12).
pub fn annual_equivalent(minor: i64, period: BillingPeriod) -> Result<i64> {
    let wide = (minor as i128) * 12;
    let annual = wide / (period.months() as i128);
    checked_output(annual, "annual_equivalent")
}

/// Monthly equivalent: `round_half_up(amount_minor / period_months)` — clearly
/// an equivalent figure, never the literal cash outflow of the coming month.
pub fn monthly_equivalent(minor: i64, period: BillingPeriod) -> Result<i64> {
    let wide = (minor as i128) * 12;
    let monthly = div_round_half_up(wide, period.months() * 12);
    checked_output(monthly, "monthly_equivalent")
}

/// One subscription contributing to a per-currency cost aggregate.
#[derive(Debug, Clone, Copy)]
pub struct CostLine {
    pub amount_minor: i64,
    pub period: BillingPeriod,
}

/// Result of folding a set of cost lines of a single currency.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CurrencyTotal {
    pub annual_equivalent_minor: i64,
    pub monthly_equivalent_minor: i64,
}

/// Aggregates exact annual figures for one currency group in `i128` with
/// checked addition, then derives a single monthly figure from the exact total.
/// Per-line rounding is deliberately avoided: it would distort the group total
/// (two 1000-minor semiannual lines: 167+167=334 vs the accurate 2000/6=333).
///
/// G2: each line may be individually legal while the sum overflows the JSON
/// safe range; that condition surfaces here as a structured error.
pub fn aggregate_currency(lines: &[CostLine]) -> Result<CurrencyTotal> {
    let mut total_annual: i128 = 0;
    for line in lines {
        let annual = (line.amount_minor as i128) * 12;
        total_annual = total_annual
            .checked_add(annual / (line.period.months() as i128))
            .ok_or_else(|| {
                Error::invalid_input("annual cost total overflowed the i128 aggregation range")
            })?;
    }
    let annual = checked_output(total_annual, "annual_equivalent_total")?;
    let monthly = checked_output(
        div_round_half_up(total_annual, 12),
        "monthly_equivalent_total",
    )?;
    Ok(CurrencyTotal {
        annual_equivalent_minor: annual,
        monthly_equivalent_minor: monthly,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn folds_monthly_annual_and_divisors() {
        assert_eq!(
            annual_equivalent(2000, BillingPeriod::Monthly).unwrap(),
            24000
        );
        assert_eq!(
            monthly_equivalent(2000, BillingPeriod::Monthly).unwrap(),
            2000
        );
        assert_eq!(
            annual_equivalent(12000, BillingPeriod::Yearly).unwrap(),
            12000
        );
        assert_eq!(
            monthly_equivalent(12000, BillingPeriod::Yearly).unwrap(),
            1000
        );
        assert_eq!(
            annual_equivalent(5000, BillingPeriod::Quarterly).unwrap(),
            20000
        );
        assert_eq!(
            monthly_equivalent(5000, BillingPeriod::Quarterly).unwrap(),
            1667
        );
        assert_eq!(
            monthly_equivalent(2500, BillingPeriod::Semiannual).unwrap(),
            417
        );
    }

    #[test]
    fn monthly_equivalent_rounds_half_up() {
        // 5000/6 = 833.33 -> 833; 2500/6 = 416.66 -> 417 (half up on the .5 edge)
        assert_eq!(
            monthly_equivalent(5000, BillingPeriod::Semiannual).unwrap(),
            833
        );
        assert_eq!(monthly_equivalent(3, BillingPeriod::Semiannual).unwrap(), 1);
    }

    #[test]
    fn g2_single_legal_input_can_overflow_annual_output() {
        // 2^53-1 monthly is a legal input but ×12 leaves the safe range.
        let err = annual_equivalent(MAX_SAFE_INTEGER, BillingPeriod::Monthly).unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "got {:?}", err);
        // Boundary that still fits: annual <= 2^53-1.
        assert_eq!(
            annual_equivalent(750_599_937_795_082, BillingPeriod::Monthly).unwrap(),
            9_007_199_253_540_984
        );
    }

    #[test]
    fn g2_aggregate_overflow_returns_structured_error() {
        // Two individually-legal annual figures whose sum exceeds 2^53-1.
        let lines = vec![
            CostLine {
                amount_minor: 4_600_000_000_000_000,
                period: BillingPeriod::Yearly,
            },
            CostLine {
                amount_minor: 4_600_000_000_000_000,
                period: BillingPeriod::Yearly,
            },
        ];
        let err = aggregate_currency(&lines).unwrap_err();
        assert!(matches!(err, Error::InvalidInput(_)), "got {:?}", err);
    }

    #[test]
    fn aggregate_rounds_once_from_exact_total() {
        // Two CNY 1000-minor semiannual subscriptions: 2000/6 = 333.33 -> 333,
        // not 167 + 167 = 334.
        let lines = vec![
            CostLine {
                amount_minor: 1000,
                period: BillingPeriod::Semiannual,
            },
            CostLine {
                amount_minor: 1000,
                period: BillingPeriod::Semiannual,
            },
        ];
        let total = aggregate_currency(&lines).unwrap();
        assert_eq!(total.monthly_equivalent_minor, 333);
        assert_eq!(total.annual_equivalent_minor, 4000);
    }

    #[test]
    fn currency_validation() {
        assert!(validate_currency("USD").is_ok());
        assert!(validate_currency("JPY").is_ok());
        assert!(validate_currency("KWD").is_ok());
        assert_eq!(currency_exponent("KWD"), Some(3));
        assert_eq!(currency_exponent("USD"), Some(2));
        assert!(validate_currency("usd").is_err());
        assert!(validate_currency("USDT").is_err());
        assert!(validate_currency("XXX").is_err());
    }

    #[test]
    fn amount_validation_rejects_zero_and_beyond_safe_range() {
        assert!(validate_amount_minor(0).is_err());
        assert!(validate_amount_minor(-5).is_err());
        assert!(validate_amount_minor(MAX_SAFE_INTEGER + 1).is_err());
        assert!(validate_amount_minor(1).is_ok());
        assert!(validate_amount_minor(MAX_SAFE_INTEGER).is_ok());
    }
}
