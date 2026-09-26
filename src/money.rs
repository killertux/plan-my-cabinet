//! Project-local money with checked minor units and no exchange-rate conversion.
//!
//! Initial currency policy: BRL and USD each have two decimal places (cents).
//! Adding currencies with different precision requires an explicit policy change;
//! amounts are never rescaled merely because a locale or currency changes.

use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum Currency {
    Brl,
    Usd,
}

impl Currency {
    pub const fn code(self) -> &'static str {
        match self {
            Self::Brl => "BRL",
            Self::Usd => "USD",
        }
    }

    pub const fn decimal_places(self) -> u32 {
        2
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoneyError {
    InvalidAmount,
    NegativeAmount,
    Overflow,
    CurrencyMismatch,
    InvalidStockIndex,
    DuplicateStock,
    ReplacementCountMismatch,
    MissingReplacement,
}

impl fmt::Display for MoneyError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for MoneyError {}

/// Non-negative monetary value in the stated currency's minor units.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct Money {
    currency: Currency,
    minor_units: i64,
}

impl Money {
    pub fn new(currency: Currency, minor_units: i64) -> Result<Self, MoneyError> {
        if minor_units < 0 {
            return Err(MoneyError::NegativeAmount);
        }
        Ok(Self {
            currency,
            minor_units,
        })
    }

    pub const fn currency(self) -> Currency {
        self.currency
    }

    pub const fn minor_units(self) -> i64 {
        self.minor_units
    }

    pub fn checked_add(self, other: Self) -> Result<Self, MoneyError> {
        if self.currency != other.currency {
            return Err(MoneyError::CurrencyMismatch);
        }
        Self::new(
            self.currency,
            self.minor_units
                .checked_add(other.minor_units)
                .ok_or(MoneyError::Overflow)?,
        )
    }

    pub fn checked_mul(self, count: u64) -> Result<Self, MoneyError> {
        let result = i128::from(self.minor_units)
            .checked_mul(i128::from(count))
            .ok_or(MoneyError::Overflow)?;
        Self::new(
            self.currency,
            i64::try_from(result).map_err(|_| MoneyError::Overflow)?,
        )
    }

    /// Parses ungrouped decimal input with either `.` or `,`, requiring exact cents.
    /// Thus `1,200` has excess precision; it never means 1,200 whole units.
    pub fn parse(currency: Currency, text: &str) -> Result<Self, MoneyError> {
        let text = text.trim();
        let text = text.strip_prefix('+').unwrap_or(text);
        if text.starts_with('-') {
            return Err(MoneyError::NegativeAmount);
        }
        let mut parts = text.split(['.', ',']);
        let whole = parts.next().ok_or(MoneyError::InvalidAmount)?;
        let fraction = parts.next();
        if whole.is_empty()
            || !whole.bytes().all(|b| b.is_ascii_digit())
            || fraction.is_some_and(|part| {
                part.is_empty()
                    || part.len() > currency.decimal_places() as usize
                    || !part.bytes().all(|b| b.is_ascii_digit())
            })
            || parts.next().is_some()
        {
            return Err(MoneyError::InvalidAmount);
        }
        let scale = 10_i64.pow(currency.decimal_places());
        let whole = whole.parse::<i64>().map_err(|_| MoneyError::Overflow)?;
        let fractional = fraction.unwrap_or("");
        let cents = if fractional.is_empty() {
            0
        } else {
            fractional
                .parse::<i64>()
                .map_err(|_| MoneyError::Overflow)?
                * 10_i64.pow(currency.decimal_places() - fractional.len() as u32)
        };
        Self::new(
            currency,
            whole
                .checked_mul(scale)
                .and_then(|v| v.checked_add(cents))
                .ok_or(MoneyError::Overflow)?,
        )
    }

    /// Locale only changes the decimal separator and cannot mutate the amount.
    pub fn display(self, locale: MoneyLocale) -> String {
        let scale = 10_i64.pow(self.currency.decimal_places());
        let separator = match locale {
            MoneyLocale::English => '.',
            MoneyLocale::PortugueseBrazil => ',',
        };
        format!(
            "{} {}{}{:02}",
            self.currency.code(),
            self.minor_units / scale,
            separator,
            self.minor_units % scale
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MoneyLocale {
    English,
    PortugueseBrazil,
}

/// An explicit project-wide currency decision. Replacement must provide a new
/// amount for every previously known price/charge; unknown entries may remain unknown.
pub enum CurrencyChange {
    ConfirmRelabelWithoutConversion,
    Replace {
        cut_charge: Option<Money>,
        stock_prices: Vec<Option<Money>>,
    },
}

/// A project pricing ledger. `None` is unknown, never a free/zero value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProjectMoney {
    currency: Currency,
    cut_charge: Option<Money>,
    stock_prices: Vec<Option<Money>>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Estimate {
    pub material: Option<Money>,
    pub cutting: Option<Money>,
    /// Present only when every required price and the cut charge is known.
    pub total: Option<Money>,
}

impl ProjectMoney {
    pub fn new(currency: Currency, stock_count: usize) -> Self {
        Self {
            currency,
            cut_charge: None,
            stock_prices: vec![None; stock_count],
        }
    }

    pub const fn currency(&self) -> Currency {
        self.currency
    }

    pub const fn cut_charge(&self) -> Option<Money> {
        self.cut_charge
    }

    pub fn stock_prices(&self) -> &[Option<Money>] {
        &self.stock_prices
    }

    fn validate(&self, amount: Option<Money>) -> Result<(), MoneyError> {
        if amount.is_some_and(|value| value.currency != self.currency) {
            return Err(MoneyError::CurrencyMismatch);
        }
        Ok(())
    }

    pub fn set_cut_charge(&mut self, amount: Option<Money>) -> Result<(), MoneyError> {
        self.validate(amount)?;
        self.cut_charge = amount;
        Ok(())
    }

    pub fn set_stock_price(
        &mut self,
        index: usize,
        amount: Option<Money>,
    ) -> Result<(), MoneyError> {
        self.validate(amount)?;
        let slot = self
            .stock_prices
            .get_mut(index)
            .ok_or(MoneyError::InvalidStockIndex)?;
        *slot = amount;
        Ok(())
    }

    /// Validates the full change before modifying any project amounts.
    pub fn change_currency(
        &mut self,
        target: Currency,
        decision: CurrencyChange,
    ) -> Result<(), MoneyError> {
        let (cut_charge, stock_prices) = match decision {
            CurrencyChange::ConfirmRelabelWithoutConversion => {
                let relabel = |amount: Option<Money>| {
                    amount.map(|value| Money {
                        currency: target,
                        minor_units: value.minor_units,
                    })
                };
                (
                    relabel(self.cut_charge),
                    self.stock_prices.iter().copied().map(relabel).collect(),
                )
            }
            CurrencyChange::Replace {
                cut_charge,
                stock_prices,
            } => {
                if stock_prices.len() != self.stock_prices.len() {
                    return Err(MoneyError::ReplacementCountMismatch);
                }
                if self.cut_charge.is_some() && cut_charge.is_none()
                    || self
                        .stock_prices
                        .iter()
                        .zip(&stock_prices)
                        .any(|(old, new)| old.is_some() && new.is_none())
                {
                    return Err(MoneyError::MissingReplacement);
                }
                if cut_charge.is_some_and(|v| v.currency != target)
                    || stock_prices.iter().flatten().any(|v| v.currency != target)
                {
                    return Err(MoneyError::CurrencyMismatch);
                }
                (cut_charge, stock_prices)
            }
        };
        self.currency = target;
        self.cut_charge = cut_charge;
        self.stock_prices = stock_prices;
        Ok(())
    }

    /// Used stock entries are (stock index, to-purchase). Owned stock costs zero;
    /// cut charges apply to all physical cuts regardless of ownership.
    pub fn estimate(
        &self,
        used_stock: &[(usize, bool)],
        cuts: u64,
    ) -> Result<Estimate, MoneyError> {
        let zero = Money::new(self.currency, 0)?;
        let mut material = Some(zero);
        let mut seen = HashSet::new();
        for &(index, to_purchase) in used_stock {
            if !seen.insert(index) {
                return Err(MoneyError::DuplicateStock);
            }
            let price = *self
                .stock_prices
                .get(index)
                .ok_or(MoneyError::InvalidStockIndex)?;
            if to_purchase {
                material = match (material, price) {
                    (Some(sum), Some(price)) => Some(sum.checked_add(price)?),
                    _ => None,
                };
            }
        }
        let cutting = if cuts == 0 {
            Some(zero)
        } else {
            self.cut_charge
                .map(|charge| charge.checked_mul(cuts))
                .transpose()?
        };
        let total = match (material, cutting) {
            (Some(material), Some(cutting)) => Some(material.checked_add(cutting)?),
            _ => None,
        };
        Ok(Estimate {
            material,
            cutting,
            total,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn brl(units: i64) -> Money {
        Money::new(Currency::Brl, units).unwrap()
    }

    fn usd(units: i64) -> Money {
        Money::new(Currency::Usd, units).unwrap()
    }

    #[test]
    fn owned_stock_purchased_sheet_and_six_paid_cuts() {
        let mut project = ProjectMoney::new(Currency::Brl, 2);
        project.set_stock_price(1, Some(brl(20_000))).unwrap();
        project.set_cut_charge(Some(brl(500))).unwrap();
        assert_eq!(
            project.estimate(&[(0, false), (1, true)], 6).unwrap(),
            Estimate {
                material: Some(brl(20_000)),
                cutting: Some(brl(3_000)),
                total: Some(brl(23_000)),
            }
        );
    }

    #[test]
    fn missing_price_and_zero_are_different() {
        let mut project = ProjectMoney::new(Currency::Brl, 1);
        project.set_cut_charge(Some(brl(500))).unwrap();
        let estimate = project.estimate(&[(0, true)], 6).unwrap();
        assert_eq!(estimate.material, None);
        assert_eq!(estimate.cutting, Some(brl(3_000)));
        assert_eq!(estimate.total, None);
        project.set_stock_price(0, Some(brl(0))).unwrap();
        assert_eq!(
            project.estimate(&[(0, true)], 6).unwrap().total,
            Some(brl(3_000))
        );
        project.set_cut_charge(None).unwrap();
        assert_eq!(project.estimate(&[(0, true)], 6).unwrap().total, None);
        assert_eq!(
            project.estimate(&[(0, true)], 0).unwrap().total,
            Some(brl(0))
        );
        assert_eq!(
            project.estimate(&[(0, true), (0, true)], 0),
            Err(MoneyError::DuplicateStock)
        );
    }

    #[test]
    fn locale_only_changes_display_and_decimal_input_is_exact() {
        let price = Money::parse(Currency::Brl, "200,05").unwrap();
        assert_eq!(price, brl(20_005));
        assert_eq!(price.display(MoneyLocale::English), "BRL 200.05");
        assert_eq!(price.display(MoneyLocale::PortugueseBrazil), "BRL 200,05");
        assert_eq!(price.minor_units(), 20_005);
        assert_eq!(
            Money::parse(Currency::Brl, "1,200"),
            Err(MoneyError::InvalidAmount)
        );
        assert_eq!(
            Money::parse(Currency::Usd, "1.234,5"),
            Err(MoneyError::InvalidAmount)
        );
        assert_eq!(
            Money::parse(Currency::Brl, "-1"),
            Err(MoneyError::NegativeAmount)
        );
        assert_eq!(
            Money::new(Currency::Brl, -1),
            Err(MoneyError::NegativeAmount)
        );
    }

    #[test]
    fn mismatches_and_overflows_do_not_change_existing_prices() {
        let mut project = ProjectMoney::new(Currency::Brl, 1);
        project.set_stock_price(0, Some(brl(10))).unwrap();
        assert_eq!(
            project.set_stock_price(0, Some(usd(10))),
            Err(MoneyError::CurrencyMismatch)
        );
        assert_eq!(
            project.set_cut_charge(Some(usd(10))),
            Err(MoneyError::CurrencyMismatch)
        );
        assert_eq!(project.stock_prices(), &[Some(brl(10))]);
        assert_eq!(
            brl(1).checked_add(usd(1)),
            Err(MoneyError::CurrencyMismatch)
        );
        assert_eq!(brl(i64::MAX).checked_add(brl(1)), Err(MoneyError::Overflow));
        assert_eq!(brl(i64::MAX).checked_mul(2), Err(MoneyError::Overflow));
        assert_eq!(
            Money::parse(Currency::Brl, "92233720368547758.08"),
            Err(MoneyError::Overflow)
        );
        project.set_cut_charge(Some(brl(i64::MAX))).unwrap();
        assert_eq!(project.estimate(&[(0, true)], 2), Err(MoneyError::Overflow));
    }

    #[test]
    fn currency_change_requires_complete_replacements_or_confirmed_relabel() {
        let mut project = ProjectMoney::new(Currency::Brl, 2);
        project.set_stock_price(0, Some(brl(20_000))).unwrap();
        project.set_cut_charge(Some(brl(500))).unwrap();
        let before = project.clone();
        assert_eq!(
            project.change_currency(
                Currency::Usd,
                CurrencyChange::Replace {
                    cut_charge: Some(usd(500)),
                    stock_prices: vec![None, None]
                }
            ),
            Err(MoneyError::MissingReplacement)
        );
        assert_eq!(project, before);
        assert_eq!(
            project.change_currency(
                Currency::Usd,
                CurrencyChange::Replace {
                    cut_charge: Some(usd(500)),
                    stock_prices: vec![Some(brl(20_000)), None]
                }
            ),
            Err(MoneyError::CurrencyMismatch)
        );
        assert_eq!(project, before);
        assert_eq!(
            project.change_currency(
                Currency::Usd,
                CurrencyChange::Replace {
                    cut_charge: Some(usd(500)),
                    stock_prices: vec![Some(usd(20_000))]
                }
            ),
            Err(MoneyError::ReplacementCountMismatch)
        );
        assert_eq!(project, before);
        project
            .change_currency(
                Currency::Usd,
                CurrencyChange::ConfirmRelabelWithoutConversion,
            )
            .unwrap();
        assert_eq!(project.stock_prices(), &[Some(usd(20_000)), None]);
        assert_eq!(project.cut_charge(), Some(usd(500)));
        project
            .change_currency(
                Currency::Brl,
                CurrencyChange::Replace {
                    cut_charge: Some(brl(750)),
                    stock_prices: vec![Some(brl(30_000)), Some(brl(0))],
                },
            )
            .unwrap();
        assert_eq!(project.stock_prices(), &[Some(brl(30_000)), Some(brl(0))]);
        assert_eq!(project.cut_charge(), Some(brl(750)));
    }
}
