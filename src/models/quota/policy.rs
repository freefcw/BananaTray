use serde::{Deserialize, Serialize};

use super::StatusLevel;
use crate::models::ProviderId;

const QUOTA_THRESHOLD_RELATIVE_EPSILON: f64 = 1e-12;
const QUOTA_CALCULATION_ROUNDING_FACTOR: f64 = 8.0;

fn remaining_at_or_below(measurement: QuotaMeasurement, threshold: f64) -> bool {
    let tolerance = (threshold.abs() * QUOTA_THRESHOLD_RELATIVE_EPSILON)
        .max(measurement.comparison_scale * f64::EPSILON * QUOTA_CALCULATION_ROUNDING_FACTOR);
    measurement.remaining <= threshold || measurement.remaining - threshold <= tolerance
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum QuotaThresholdUnit {
    Percentage,
    Currency,
    Amount,
}

impl QuotaThresholdUnit {
    pub const ALL: [Self; 3] = [Self::Percentage, Self::Currency, Self::Amount];

    pub fn config_key(self) -> &'static str {
        match self {
            Self::Percentage => "percentage",
            Self::Currency => "currency",
            Self::Amount => "amount",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QuotaThresholds {
    pub warning: f64,
    pub critical: f64,
    pub notify: f64,
}

impl QuotaThresholds {
    pub const DEFAULT_PERCENTAGE: Self = Self {
        warning: 50.0,
        critical: 20.0,
        notify: 10.0,
    };

    pub const DEFAULT_CURRENCY: Self = Self {
        warning: 10.0,
        critical: 2.0,
        notify: 1.0,
    };

    pub const DEFAULT_AMOUNT: Self = Self {
        warning: 100.0,
        critical: 20.0,
        notify: 10.0,
    };

    pub fn default_for(unit: QuotaThresholdUnit) -> Self {
        match unit {
            QuotaThresholdUnit::Percentage => Self::DEFAULT_PERCENTAGE,
            QuotaThresholdUnit::Currency => Self::DEFAULT_CURRENCY,
            QuotaThresholdUnit::Amount => Self::DEFAULT_AMOUNT,
        }
    }

    pub fn validate(&self, unit: QuotaThresholdUnit) -> Result<(), QuotaThresholdsError> {
        for value in [self.warning, self.critical, self.notify] {
            if !value.is_finite() {
                return Err(QuotaThresholdsError::NotFinite);
            }
            if value <= 0.0 {
                return Err(QuotaThresholdsError::NotPositive);
            }
        }
        if !(self.notify <= self.critical && self.critical < self.warning) {
            return Err(QuotaThresholdsError::InvalidOrder);
        }
        if unit == QuotaThresholdUnit::Percentage && self.warning > 100.0 {
            return Err(QuotaThresholdsError::PercentageExceeds100);
        }
        Ok(())
    }

    pub fn parse(
        unit: QuotaThresholdUnit,
        warning: &str,
        critical: &str,
        notify: &str,
    ) -> Result<Self, QuotaThresholdsError> {
        let thresholds = Self {
            warning: parse_threshold_value(warning)?,
            critical: parse_threshold_value(critical)?,
            notify: parse_threshold_value(notify)?,
        };
        thresholds.validate(unit)?;
        Ok(thresholds)
    }

    pub fn status_level(&self, measurement: QuotaMeasurement) -> StatusLevel {
        if remaining_at_or_below(measurement, self.critical) {
            StatusLevel::Red
        } else if remaining_at_or_below(measurement, self.warning) {
            StatusLevel::Yellow
        } else {
            StatusLevel::Green
        }
    }

    pub fn notify_threshold_reached(&self, measurement: QuotaMeasurement) -> bool {
        remaining_at_or_below(measurement, self.notify)
    }
}

fn parse_threshold_value(raw: &str) -> Result<f64, QuotaThresholdsError> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err(QuotaThresholdsError::EmptyInput);
    }
    let value: f64 = trimmed
        .parse()
        .map_err(|_| QuotaThresholdsError::InvalidNumber)?;
    if !value.is_finite() {
        return Err(QuotaThresholdsError::NotFinite);
    }
    Ok(value)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum QuotaThresholdsError {
    EmptyInput,
    InvalidNumber,
    NotFinite,
    NotPositive,
    InvalidOrder,
    PercentageExceeds100,
}

#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct QuotaRules {
    pub percentage: QuotaThresholds,
    pub currency: QuotaThresholds,
    pub amount: QuotaThresholds,
}

impl Default for QuotaRules {
    fn default() -> Self {
        Self {
            percentage: QuotaThresholds::DEFAULT_PERCENTAGE,
            currency: QuotaThresholds::DEFAULT_CURRENCY,
            amount: QuotaThresholds::DEFAULT_AMOUNT,
        }
    }
}

impl QuotaRules {
    pub fn thresholds(&self, unit: QuotaThresholdUnit) -> QuotaThresholds {
        match unit {
            QuotaThresholdUnit::Percentage => self.percentage,
            QuotaThresholdUnit::Currency => self.currency,
            QuotaThresholdUnit::Amount => self.amount,
        }
    }

    pub fn set(&mut self, unit: QuotaThresholdUnit, thresholds: QuotaThresholds) {
        match unit {
            QuotaThresholdUnit::Percentage => self.percentage = thresholds,
            QuotaThresholdUnit::Currency => self.currency = thresholds,
            QuotaThresholdUnit::Amount => self.amount = thresholds,
        }
    }

    pub fn resolve(&self, overrides: &QuotaRuleOverrides) -> Self {
        let mut resolved = *self;
        for unit in QuotaThresholdUnit::ALL {
            if let Some(thresholds) = overrides.get(unit) {
                resolved.set(unit, thresholds);
            }
        }
        resolved
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
#[serde(default)]
pub struct QuotaRuleOverrides {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub percentage: Option<QuotaThresholds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub currency: Option<QuotaThresholds>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub amount: Option<QuotaThresholds>,
}

impl QuotaRuleOverrides {
    pub fn get(&self, unit: QuotaThresholdUnit) -> Option<QuotaThresholds> {
        match unit {
            QuotaThresholdUnit::Percentage => self.percentage,
            QuotaThresholdUnit::Currency => self.currency,
            QuotaThresholdUnit::Amount => self.amount,
        }
    }

    pub fn set(&mut self, unit: QuotaThresholdUnit, thresholds: Option<QuotaThresholds>) {
        match unit {
            QuotaThresholdUnit::Percentage => self.percentage = thresholds,
            QuotaThresholdUnit::Currency => self.currency = thresholds,
            QuotaThresholdUnit::Amount => self.amount = thresholds,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.percentage.is_none() && self.currency.is_none() && self.amount.is_none()
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QuotaMeasurement {
    pub unit: QuotaThresholdUnit,
    pub remaining: f64,
    pub comparison_scale: f64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum QuotaThresholdTarget {
    Global,
    Provider(ProviderId),
}

impl QuotaThresholdTarget {
    pub fn is_provider(&self, id: &ProviderId) -> bool {
        matches!(self, Self::Provider(owner) if owner == id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn valid(warning: f64, critical: f64, notify: f64) -> QuotaThresholds {
        QuotaThresholds {
            warning,
            critical,
            notify,
        }
    }

    fn measurement(unit: QuotaThresholdUnit, remaining: f64) -> QuotaMeasurement {
        QuotaMeasurement {
            unit,
            remaining,
            comparison_scale: remaining.abs(),
        }
    }

    #[test]
    fn default_rules_match_fixed_thresholds() {
        let rules = QuotaRules::default();
        assert_eq!(rules.percentage, QuotaThresholds::DEFAULT_PERCENTAGE);
        assert_eq!(rules.currency, QuotaThresholds::DEFAULT_CURRENCY);
        assert_eq!(rules.amount, QuotaThresholds::DEFAULT_AMOUNT);
        assert_eq!(QuotaThresholds::DEFAULT_PERCENTAGE, valid(50.0, 20.0, 10.0));
        assert_eq!(QuotaThresholds::DEFAULT_CURRENCY, valid(10.0, 2.0, 1.0));
        assert_eq!(QuotaThresholds::DEFAULT_AMOUNT, valid(100.0, 20.0, 10.0));
    }

    #[test]
    fn status_level_uses_inclusive_remaining_boundaries() {
        let t = QuotaThresholds::DEFAULT_PERCENTAGE;
        assert_eq!(
            t.status_level(measurement(QuotaThresholdUnit::Percentage, 50.0)),
            StatusLevel::Yellow
        );
        assert_eq!(
            t.status_level(measurement(QuotaThresholdUnit::Percentage, 50.1)),
            StatusLevel::Green
        );
        assert_eq!(
            t.status_level(measurement(QuotaThresholdUnit::Percentage, 20.0)),
            StatusLevel::Red
        );
        assert_eq!(
            t.status_level(measurement(QuotaThresholdUnit::Percentage, 0.0)),
            StatusLevel::Red
        );
        assert_eq!(
            t.status_level(measurement(QuotaThresholdUnit::Percentage, -3.0)),
            StatusLevel::Red
        );
    }

    #[test]
    fn validate_accepts_boundary_order() {
        let t = valid(10.0, 1.0, 1.0);
        assert!(t.validate(QuotaThresholdUnit::Currency).is_ok());
        let t = valid(100.0, 20.0, 10.0);
        assert!(t.validate(QuotaThresholdUnit::Percentage).is_ok());
    }

    #[test]
    fn validate_rejects_non_finite_and_non_positive() {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let t = QuotaThresholds {
                warning: bad,
                critical: 20.0,
                notify: 10.0,
            };
            assert_eq!(
                t.validate(QuotaThresholdUnit::Percentage),
                Err(QuotaThresholdsError::NotFinite)
            );
        }
        for bad in [0.0, -1.0] {
            let t = QuotaThresholds {
                warning: 50.0,
                critical: 20.0,
                notify: bad,
            };
            assert_eq!(
                t.validate(QuotaThresholdUnit::Percentage),
                Err(QuotaThresholdsError::NotPositive)
            );
        }
    }

    #[test]
    fn validate_rejects_reversed_order() {
        let t = valid(50.0, 20.0, 30.0);
        assert_eq!(
            t.validate(QuotaThresholdUnit::Percentage),
            Err(QuotaThresholdsError::InvalidOrder)
        );
        let t = valid(20.0, 20.0, 10.0);
        assert_eq!(
            t.validate(QuotaThresholdUnit::Percentage),
            Err(QuotaThresholdsError::InvalidOrder)
        );
    }

    #[test]
    fn validate_rejects_percentage_warning_over_100() {
        let t = valid(120.0, 20.0, 10.0);
        assert_eq!(
            t.validate(QuotaThresholdUnit::Percentage),
            Err(QuotaThresholdsError::PercentageExceeds100)
        );
        assert!(t.validate(QuotaThresholdUnit::Currency).is_ok());
    }

    #[test]
    fn parse_rejects_invalid_inputs() {
        let unit = QuotaThresholdUnit::Percentage;
        assert_eq!(
            QuotaThresholds::parse(unit, "", "20", "10"),
            Err(QuotaThresholdsError::EmptyInput)
        );
        assert_eq!(
            QuotaThresholds::parse(unit, "  ", "20", "10"),
            Err(QuotaThresholdsError::EmptyInput)
        );
        assert_eq!(
            QuotaThresholds::parse(unit, "abc", "20", "10"),
            Err(QuotaThresholdsError::InvalidNumber)
        );
        assert_eq!(
            QuotaThresholds::parse(unit, "NaN", "20", "10"),
            Err(QuotaThresholdsError::NotFinite)
        );
        assert_eq!(
            QuotaThresholds::parse(unit, "inf", "20", "10"),
            Err(QuotaThresholdsError::NotFinite)
        );
        assert_eq!(
            QuotaThresholds::parse(unit, "50", "20", "0"),
            Err(QuotaThresholdsError::NotPositive)
        );
        assert_eq!(
            QuotaThresholds::parse(unit, "50", "20", "-5"),
            Err(QuotaThresholdsError::NotPositive)
        );
        assert_eq!(
            QuotaThresholds::parse(unit, "50", "10", "20"),
            Err(QuotaThresholdsError::InvalidOrder)
        );
        assert_eq!(
            QuotaThresholds::parse(unit, "101", "20", "10"),
            Err(QuotaThresholdsError::PercentageExceeds100)
        );
        assert!(QuotaThresholds::parse(unit, " 50 ", " 20 ", " 10 ").is_ok());
    }

    #[test]
    fn resolve_overrides_per_unit() {
        let mut overrides = QuotaRuleOverrides::default();
        let custom = valid(20.0, 5.0, 2.0);
        overrides.set(QuotaThresholdUnit::Currency, Some(custom));

        let resolved = QuotaRules::default().resolve(&overrides);
        assert_eq!(resolved.currency, custom);
        assert_eq!(resolved.percentage, QuotaThresholds::DEFAULT_PERCENTAGE);
        assert_eq!(resolved.amount, QuotaThresholds::DEFAULT_AMOUNT);
        assert!(!overrides.is_empty());
        overrides.set(QuotaThresholdUnit::Currency, None);
        assert!(overrides.is_empty());
    }

    #[test]
    fn threshold_target_matches_only_owning_provider() {
        let claude = ProviderId::BuiltIn(crate::models::ProviderKind::Claude);
        let codex = ProviderId::BuiltIn(crate::models::ProviderKind::Codex);
        let target = QuotaThresholdTarget::Provider(claude.clone());

        assert!(target.is_provider(&claude));
        assert!(!target.is_provider(&codex));
        assert!(!QuotaThresholdTarget::Global.is_provider(&claude));
    }

    #[test]
    fn threshold_boundaries_use_relative_tolerance() {
        let t = valid(0.5, 0.1, 0.1);
        let reconstructed = QuotaMeasurement {
            unit: QuotaThresholdUnit::Currency,
            remaining: 20.3 - (20.3 - 0.1),
            comparison_scale: 20.3,
        };
        assert_eq!(t.status_level(reconstructed), StatusLevel::Red);
        assert!(t.notify_threshold_reached(reconstructed));

        assert_eq!(
            t.status_level(measurement(QuotaThresholdUnit::Currency, 0.5 + 1e-14)),
            StatusLevel::Yellow
        );

        assert_eq!(
            t.status_level(measurement(QuotaThresholdUnit::Currency, 0.1 + 1e-10)),
            StatusLevel::Yellow
        );
        assert!(!t.notify_threshold_reached(measurement(QuotaThresholdUnit::Currency, 0.1 + 1e-10)));
        assert_eq!(
            t.status_level(measurement(QuotaThresholdUnit::Currency, 0.5 + 1e-10)),
            StatusLevel::Green
        );
    }

    #[test]
    fn threshold_boundaries_scale_with_operand_magnitude() {
        let t = valid(0.5, 0.01, 0.01);
        let one_cent = QuotaMeasurement {
            unit: QuotaThresholdUnit::Currency,
            remaining: 0.010000000000019327,
            comparison_scale: 200.02,
        };
        assert_eq!(t.status_level(one_cent), StatusLevel::Red);
        assert!(t.notify_threshold_reached(one_cent));

        let beyond = QuotaMeasurement {
            unit: QuotaThresholdUnit::Currency,
            remaining: 0.01 + 1e-10,
            comparison_scale: 200.02,
        };
        assert_eq!(t.status_level(beyond), StatusLevel::Yellow);
        assert!(!t.notify_threshold_reached(beyond));
    }

    #[test]
    fn tiny_positive_thresholds_have_no_absolute_epsilon_floor() {
        let t = valid(5e-12, 2e-12, 1e-12);
        assert_eq!(
            t.status_level(measurement(QuotaThresholdUnit::Amount, 3e-12)),
            StatusLevel::Yellow
        );
        assert!(!t.notify_threshold_reached(measurement(QuotaThresholdUnit::Amount, 3e-12)));
    }

    #[test]
    fn validate_stays_strict_at_epsilon_scale() {
        assert!(valid(0.5, 0.5 - 1e-15, 0.1)
            .validate(QuotaThresholdUnit::Currency)
            .is_ok());
        assert_eq!(
            valid(0.5, 0.5, 0.1).validate(QuotaThresholdUnit::Currency),
            Err(QuotaThresholdsError::InvalidOrder)
        );
    }
}
