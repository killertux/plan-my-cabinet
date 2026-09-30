//! JSON shapes shared by the agent tools: lengths (numbers are millimetres,
//! strings may carry a unit), money, poses, object references and the
//! snake_case enums that mirror the domain.
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use crate::dimension_input::{Locale, format_length, parse_length};
use crate::domain::{BoardGrain, StockGrain, StockSource};
use crate::money::{Currency, Money};
use crate::placement::{self, euler_degrees_xyz};
use crate::service::error::{ErrorCode, ServiceError, ServiceResult};
use crate::units::{Anchor, Conversion, Length, Pose, Unit};

/// Distinguish an omitted field (`None`) from an explicit null (`Some(None)`).
pub fn double_option<'de, T, D>(deserializer: D) -> Result<Option<Option<T>>, D::Error>
where
    T: Deserialize<'de>,
    D: serde::Deserializer<'de>,
{
    Option::<T>::deserialize(deserializer).map(Some)
}

/// "FooBar" → "foo_bar".
pub fn snake(text: &str) -> String {
    let mut out = String::new();
    for (i, c) in text.chars().enumerate() {
        if c.is_uppercase() {
            if i > 0 {
                out.push('_');
            }
            out.extend(c.to_lowercase());
        } else {
            out.push(c);
        }
    }
    out
}

/// A length: a number is millimetres; a string is parsed like the app's
/// dimension fields ("600", "60 cm", "0.6 m", "23 5/8 in", "2'", "12,5").
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum LengthInput {
    Millimetres(f64),
    Text(String),
}

impl LengthInput {
    /// Resolve to exact micrometres. A value that is not a whole micrometre is
    /// refused unless `allow_rounding`.
    pub fn resolve(&self, field: &str, allow_rounding: bool) -> ServiceResult<Length> {
        let conversion = match self {
            Self::Millimetres(mm) => {
                if !mm.is_finite() {
                    return Err(
                        ServiceError::new(ErrorCode::InvalidLength, "not a finite number")
                            .for_field(field),
                    );
                }
                let um = mm * 1000.0;
                if um.abs() > 9.0e15 {
                    return Err(ServiceError::new(
                        ErrorCode::InvalidLength,
                        "the value is too large",
                    )
                    .for_field(field));
                }
                let rounded = um.round();
                if (um - rounded).abs() < 1e-6 {
                    Conversion::Exact(Length::from_micrometres(rounded as i64))
                } else {
                    Conversion::NeedsConfirmation(Length::from_micrometres(rounded as i64))
                }
            }
            Self::Text(text) => {
                parse_length(text, Unit::Mm)
                    .map_err(|e| ServiceError::from(e).for_field(field))?
                    .conversion
            }
        };
        match conversion {
            Conversion::Exact(value) => Ok(value),
            Conversion::NeedsConfirmation(value) if allow_rounding => Ok(value),
            Conversion::NeedsConfirmation(value) => Err(ServiceError::new(
                ErrorCode::RoundingRequired,
                format!(
                    "{field}: the value is not a whole micrometre; it would be rounded to {} mm",
                    mm_f64(value)
                ),
            )
            .hint("Pass allow_rounding: true to accept the rounded value, or give an exact value.")
            .details(serde_json::json!({ "field": field, "suggested_mm": mm_f64(value) }))),
        }
    }

    pub fn positive(&self, field: &str, allow_rounding: bool) -> ServiceResult<Length> {
        let value = self.resolve(field, allow_rounding)?;
        if value.micrometres() <= 0 {
            return Err(ServiceError::new(
                ErrorCode::InvalidLength,
                format!("{field} must be greater than zero"),
            ));
        }
        Ok(value)
    }

    pub fn non_negative(&self, field: &str, allow_rounding: bool) -> ServiceResult<Length> {
        let value = self.resolve(field, allow_rounding)?;
        if value.micrometres() < 0 {
            return Err(ServiceError::new(
                ErrorCode::InvalidLength,
                format!("{field} cannot be negative"),
            ));
        }
        Ok(value)
    }

    /// As millimetres (for poses and offsets, which are f64 millimetres).
    pub fn millimetres(&self, field: &str, allow_rounding: bool) -> ServiceResult<f64> {
        Ok(mm_f64(self.resolve(field, allow_rounding)?))
    }
}

pub fn mm_f64(value: Length) -> f64 {
    value.micrometres() as f64 / 1000.0
}

/// A length for output: exact millimetres plus text in the project unit.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct LengthOut {
    pub mm: f64,
    pub text: String,
}

impl LengthOut {
    pub fn new(value: Length, unit: Unit) -> Self {
        Self {
            mm: mm_f64(value),
            text: format_length(value, unit, Locale::En, 3),
        }
    }
}

/// Money: a string like "12.34" / "12,34" or a number, in the project currency.
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum MoneyInput {
    Amount(f64),
    Text(String),
}

impl MoneyInput {
    pub fn resolve(&self, currency: Currency) -> ServiceResult<Money> {
        let text = match self {
            Self::Amount(value) => {
                if !value.is_finite() {
                    return Err(ServiceError::invalid("the amount is not a finite number"));
                }
                format!("{value:.2}")
            }
            Self::Text(text) => text.clone(),
        };
        Ok(Money::parse(currency, text.trim())?)
    }
}

pub fn money_out(money: Option<Money>) -> Option<MoneyOut> {
    money.map(|m| MoneyOut {
        currency: currency_code(m.currency()),
        minor_units: m.minor_units(),
        text: m.display(crate::money::MoneyLocale::English),
    })
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MoneyOut {
    pub currency: &'static str,
    pub minor_units: i64,
    pub text: String,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum CurrencyCode {
    #[serde(alias = "brl")]
    BRL,
    #[serde(alias = "usd")]
    USD,
}

impl From<CurrencyCode> for Currency {
    fn from(code: CurrencyCode) -> Self {
        match code {
            CurrencyCode::BRL => Currency::Brl,
            CurrencyCode::USD => Currency::Usd,
        }
    }
}

pub fn currency_code(currency: Currency) -> &'static str {
    match currency {
        Currency::Brl => "BRL",
        Currency::Usd => "USD",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum UnitName {
    Mm,
    Cm,
    M,
    #[serde(alias = "in")]
    Inch,
    #[serde(alias = "ft")]
    Foot,
}

impl From<UnitName> for Unit {
    fn from(unit: UnitName) -> Self {
        match unit {
            UnitName::Mm => Unit::Mm,
            UnitName::Cm => Unit::Cm,
            UnitName::M => Unit::M,
            UnitName::Inch => Unit::Inch,
            UnitName::Foot => Unit::Foot,
        }
    }
}

pub fn unit_name(unit: Unit) -> &'static str {
    match unit {
        Unit::Mm => "mm",
        Unit::Cm => "cm",
        Unit::M => "m",
        Unit::Inch => "inch",
        Unit::Foot => "foot",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Grain {
    /// Grain runs along the board's length.
    Length,
    /// Grain runs along the board's width.
    Width,
    /// Either orientation is fine.
    Unrestricted,
}

impl From<Grain> for BoardGrain {
    fn from(grain: Grain) -> Self {
        match grain {
            Grain::Length => BoardGrain::Length,
            Grain::Width => BoardGrain::Width,
            Grain::Unrestricted => BoardGrain::Unrestricted,
        }
    }
}

pub fn grain_name(grain: BoardGrain) -> &'static str {
    match grain {
        BoardGrain::Length => "length",
        BoardGrain::Width => "width",
        BoardGrain::Unrestricted => "unrestricted",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum SheetGrain {
    /// Along the sheet length.
    AlongX,
    /// Along the sheet width.
    AlongY,
    Nondirectional,
    Unknown,
}

impl From<SheetGrain> for StockGrain {
    fn from(grain: SheetGrain) -> Self {
        match grain {
            SheetGrain::AlongX => StockGrain::AlongX,
            SheetGrain::AlongY => StockGrain::AlongY,
            SheetGrain::Nondirectional => StockGrain::Nondirectional,
            SheetGrain::Unknown => StockGrain::Unknown,
        }
    }
}

pub fn sheet_grain_name(grain: StockGrain) -> &'static str {
    match grain {
        StockGrain::AlongX => "along_x",
        StockGrain::AlongY => "along_y",
        StockGrain::Nondirectional => "nondirectional",
        StockGrain::Unknown => "unknown",
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Source {
    /// Already in the shop.
    Owned,
    /// Must be bought (counts as new spending).
    ToPurchase,
}

impl From<Source> for StockSource {
    fn from(source: Source) -> Self {
        match source {
            Source::Owned => StockSource::Owned,
            Source::ToPurchase => StockSource::ToPurchase,
        }
    }
}

pub fn source_name(source: StockSource) -> &'static str {
    match source {
        StockSource::Owned => "owned",
        StockSource::ToPurchase => "to_purchase",
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AnchorName {
    /// Keep the low (origin) face fixed.
    #[default]
    Start,
    /// Keep the centre fixed.
    #[serde(alias = "center")]
    Centre,
    /// Keep the high face fixed.
    End,
}

impl From<AnchorName> for Anchor {
    fn from(anchor: AnchorName) -> Self {
        match anchor {
            AnchorName::Start => Anchor::Start,
            AnchorName::Centre => Anchor::Centre,
            AnchorName::End => Anchor::End,
        }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum Frame {
    /// Relative to the parent assembly (the stored pose).
    #[default]
    Parent,
    World,
}

/// A position in millimetres and a rotation in degrees (intrinsic X, then Y,
/// then Z — the app's numeric position dialog). Omitted values are 0.
#[derive(Clone, Copy, Debug, Default, PartialEq, Deserialize, Serialize, JsonSchema)]
pub struct PoseInput {
    #[serde(default)]
    pub x: f64,
    #[serde(default)]
    pub y: f64,
    #[serde(default)]
    pub z: f64,
    #[serde(default)]
    pub rx: f64,
    #[serde(default)]
    pub ry: f64,
    #[serde(default)]
    pub rz: f64,
}

impl PoseInput {
    pub fn pose(&self) -> ServiceResult<Pose> {
        let rotation = placement::rotation_from_degrees_xyz([self.rx, self.ry, self.rz])?;
        let position = [self.x, self.y, self.z].map(|v| (v * 1000.0).round() / 1000.0);
        Ok(Pose::new(position, rotation)?)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum PosePresetName {
    /// Length points up (+Z), thickness toward -X: a side panel.
    StandUp,
    /// Local axes equal to the frame axes: a bottom, top or shelf.
    LayFlat,
    /// Turn 90° about the frame's Z axis.
    Turn90Z,
}

impl From<PosePresetName> for placement::PosePreset {
    fn from(preset: PosePresetName) -> Self {
        match preset {
            PosePresetName::StandUp => placement::PosePreset::StandUp,
            PosePresetName::LayFlat => placement::PosePreset::LayFlat,
            PosePresetName::Turn90Z => placement::PosePreset::Turn90Z,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Euler {
    pub x: f64,
    pub y: f64,
    pub z: f64,
    pub rx: f64,
    pub ry: f64,
    pub rz: f64,
}

fn r3(v: f64) -> f64 {
    let r = (v * 1000.0).round() / 1000.0;
    if r == 0.0 { 0.0 } else { r }
}

impl From<Pose> for Euler {
    fn from(pose: Pose) -> Self {
        let [rx, ry, rz] = euler_degrees_xyz(pose.rotation).map(|a| (a * 1e6).round() / 1e6);
        Self {
            x: r3(pose.translation_mm[0]),
            y: r3(pose.translation_mm[1]),
            z: r3(pose.translation_mm[2]),
            rx: if rx == 0.0 { 0.0 } else { rx },
            ry: if ry == 0.0 { 0.0 } else { ry },
            rz: if rz == 0.0 { 0.0 } else { rz },
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct PoseOut {
    /// Relative to the parent assembly.
    pub local: Euler,
    pub world: Euler,
}

/// A board-local face: x = length, y = width, z = thickness.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
pub enum FaceName {
    #[serde(rename = "-x")]
    MinusX,
    #[serde(rename = "+x")]
    PlusX,
    #[serde(rename = "-y")]
    MinusY,
    #[serde(rename = "+y")]
    PlusY,
    #[serde(rename = "-z")]
    MinusZ,
    #[serde(rename = "+z")]
    PlusZ,
}

impl From<FaceName> for placement::BoardFace {
    fn from(face: FaceName) -> Self {
        use placement::Side::{Negative, Positive};
        let (axis, side) = match face {
            FaceName::MinusX => (0, Negative),
            FaceName::PlusX => (0, Positive),
            FaceName::MinusY => (1, Negative),
            FaceName::PlusY => (1, Positive),
            FaceName::MinusZ => (2, Negative),
            FaceName::PlusZ => (2, Positive),
        };
        placement::BoardFace { axis, side }
    }
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Deserialize, Serialize, JsonSchema)]
#[serde(rename_all = "snake_case")]
pub enum AlignName {
    #[default]
    Start,
    #[serde(alias = "center")]
    Centre,
    End,
}

impl From<AlignName> for placement::Align {
    fn from(align: AlignName) -> Self {
        match align {
            AlignName::Start => placement::Align::Start,
            AlignName::Centre => placement::Align::Centre,
            AlignName::End => placement::Align::End,
        }
    }
}

/// Parse a color given as "#rrggbb" or [r, g, b].
#[derive(Clone, Debug, PartialEq, Deserialize, Serialize, JsonSchema)]
#[serde(untagged)]
pub enum ColorInput {
    Rgb([u8; 3]),
    Hex(String),
}

impl ColorInput {
    pub fn resolve(&self) -> ServiceResult<crate::domain::SrgbColor> {
        match self {
            Self::Rgb(rgb) => Ok(crate::domain::SrgbColor(*rgb)),
            Self::Hex(text) => {
                let hex = text.trim().trim_start_matches('#');
                if hex.len() != 6 {
                    return Err(ServiceError::invalid("colors are \"#rrggbb\" or [r, g, b]"));
                }
                let byte = |i: usize| {
                    u8::from_str_radix(&hex[i..i + 2], 16)
                        .map_err(|_| ServiceError::invalid("colors are \"#rrggbb\" or [r, g, b]"))
                };
                Ok(crate::domain::SrgbColor([byte(0)?, byte(2)?, byte(4)?]))
            }
        }
    }
}

pub fn color_hex(color: crate::domain::SrgbColor) -> String {
    let [r, g, b] = color.0;
    format!("#{r:02x}{g:02x}{b:02x}")
}

/// The common result of every change: what happened and the new revision.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct Change<T: Serialize> {
    /// False for dry runs.
    pub committed: bool,
    /// False when the request matched the current state (no undo step).
    pub changed: bool,
    pub revision: u64,
    /// Unsaved changes exist.
    pub dirty: bool,
    pub undo_steps: usize,
    pub summary: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub warnings: Vec<String>,
    #[serde(flatten)]
    pub result: T,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn length(value: serde_json::Value) -> LengthInput {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn numbers_are_millimetres_and_strings_carry_units() {
        let um = |v: serde_json::Value| length(v).resolve("x", false).map(Length::micrometres);
        assert_eq!(um(serde_json::json!(600)), Ok(600_000));
        assert_eq!(um(serde_json::json!(18.5)), Ok(18_500));
        assert_eq!(um(serde_json::json!("60 cm")), Ok(600_000));
        assert_eq!(um(serde_json::json!("12,5")), Ok(12_500));
        assert_eq!(um(serde_json::json!("2'")), Ok(609_600));
        let rounding = length(serde_json::json!("1/3 in"))
            .resolve("x", false)
            .unwrap_err();
        assert_eq!(rounding.code, ErrorCode::RoundingRequired);
        assert!(
            length(serde_json::json!("1/3 in"))
                .resolve("x", true)
                .is_ok()
        );
        let negative = length(serde_json::json!(-3))
            .positive("x", false)
            .unwrap_err();
        assert_eq!(negative.code, ErrorCode::InvalidLength);
        assert_eq!(
            length(serde_json::json!("1,234.5"))
                .resolve("x", false)
                .unwrap_err()
                .code,
            ErrorCode::InvalidLength
        );
    }

    #[test]
    fn poses_round_trip_through_degrees() {
        let input = PoseInput {
            x: 10.0,
            y: -18.0,
            z: 2.0,
            rx: 90.0,
            ry: 0.0,
            rz: 0.0,
        };
        let euler = Euler::from(input.pose().unwrap());
        assert_eq!([euler.x, euler.y, euler.z], [10.0, -18.0, 2.0]);
        assert!((euler.rx - 90.0).abs() < 1e-6 && euler.ry.abs() < 1e-6 && euler.rz.abs() < 1e-6);
    }
}
