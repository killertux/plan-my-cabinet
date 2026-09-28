//! Exact manufacturing quantities and independently bounded rigid spatial poses.
//! Board-local geometry starts at the pose origin; X/Y/Z are length/width/thickness.

use serde::{Deserialize, Serialize};
use std::fmt;

/// A signed manufacturing coordinate or length in thousandths of a millimetre.
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize,
)]
pub struct Length(i64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum Unit {
    Mm,
    Cm,
    M,
    Inch,
    Foot,
}

impl Unit {
    const fn micrometres(self) -> i128 {
        match self {
            Self::Mm => 1_000,
            Self::Cm => 10_000,
            Self::M => 1_000_000,
            Self::Inch => 25_400,
            Self::Foot => 304_800,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum UnitError {
    InvalidNumber,
    InvalidFraction,
    Overflow,
    NonPositiveDimension,
    OutOfBounds,
    NonFinite,
    InvalidRotation,
}

impl fmt::Display for UnitError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{self:?}")
    }
}

impl std::error::Error for UnitError {}

/// Exact conversion result. Only `Exact` may be committed immediately;
/// `NeedsConfirmation` exposes a half-away-from-zero grid suggestion, never a commit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Conversion {
    Exact(Length),
    NeedsConfirmation(Length),
}

impl Conversion {
    pub const fn suggested(self) -> Length {
        match self {
            Self::Exact(value) | Self::NeedsConfirmation(value) => value,
        }
    }

    pub const fn exact(self) -> Option<Length> {
        match self {
            Self::Exact(value) => Some(value),
            Self::NeedsConfirmation(_) => None,
        }
    }
}

impl Length {
    pub const ZERO: Self = Self(0);

    pub const fn from_micrometres(value: i64) -> Self {
        Self(value)
    }

    pub const fn micrometres(self) -> i64 {
        self.0
    }

    pub fn positive(self) -> Result<Self, UnitError> {
        if self.0 > 0 {
            Ok(self)
        } else {
            Err(UnitError::NonPositiveDimension)
        }
    }

    pub fn checked_add(self, other: Self) -> Result<Self, UnitError> {
        self.0
            .checked_add(other.0)
            .map(Self)
            .ok_or(UnitError::Overflow)
    }

    pub fn checked_sub(self, other: Self) -> Result<Self, UnitError> {
        self.0
            .checked_sub(other.0)
            .map(Self)
            .ok_or(UnitError::Overflow)
    }

    /// Parses an ungrouped decimal magnitude. The UI parser in task 2.2 supplies
    /// the selected unit and handles suffixes, locale and input-preview state.
    pub fn from_decimal(value: &str, unit: Unit) -> Result<Conversion, UnitError> {
        let (numerator, denominator) = decimal(value)?;
        convert(numerator, denominator, unit)
    }

    /// Accepts proper or mixed inch fractions (`3/4`, `1 1/2`), with an
    /// optional leading minus for signed coordinates. Unit suffix handling is
    /// deliberately left to the UI input parser.
    pub fn from_inch_fraction(value: &str) -> Result<Conversion, UnitError> {
        let value = value.trim();
        let (negative, unsigned) = match value.strip_prefix('-') {
            Some(rest) => (true, rest),
            None => (false, value.strip_prefix('+').unwrap_or(value)),
        };
        let parts: Vec<_> = unsigned.split_whitespace().collect();
        let (whole, fraction) = match parts.as_slice() {
            [fraction] => (0_i128, *fraction),
            [whole, fraction] => (digits(whole).ok_or(UnitError::InvalidFraction)?, *fraction),
            _ => return Err(UnitError::InvalidFraction),
        };
        let (top, bottom) = fraction.split_once('/').ok_or(UnitError::InvalidFraction)?;
        let top = digits(top).ok_or(UnitError::InvalidFraction)?;
        let bottom = digits(bottom).ok_or(UnitError::InvalidFraction)?;
        if bottom == 0 || top == 0 || top >= bottom {
            return Err(UnitError::InvalidFraction);
        }
        let numerator = whole
            .checked_mul(bottom)
            .and_then(|n| n.checked_add(top))
            .ok_or(UnitError::Overflow)?;
        convert(
            if negative { -numerator } else { numerator },
            bottom,
            Unit::Inch,
        )
    }
}

fn digits(value: &str) -> Option<i128> {
    if value.is_empty() || !value.bytes().all(|b| b.is_ascii_digit()) {
        return None;
    }
    value.parse().ok()
}

fn decimal(value: &str) -> Result<(i128, i128), UnitError> {
    let value = value.trim();
    let (sign, value) = match value.as_bytes().first() {
        Some(b'-') => (-1, &value[1..]),
        Some(b'+') => (1, &value[1..]),
        _ => (1, value),
    };
    let mut halves = value.split(['.', ',']);
    let whole = halves.next().ok_or(UnitError::InvalidNumber)?;
    let fractional = halves.next();
    if halves.next().is_some()
        || whole.is_empty()
        || fractional == Some("")
        || value.contains(char::is_whitespace)
    {
        return Err(UnitError::InvalidNumber);
    }
    let whole = digits(whole).ok_or(UnitError::InvalidNumber)?;
    let fraction = fractional.unwrap_or("");
    let frac = digits(fraction).unwrap_or(0);
    if !fraction.is_empty() && (digits(fraction).is_none() || fraction.len() > 38) {
        return Err(UnitError::InvalidNumber);
    }
    let denominator = 10_i128
        .checked_pow(fraction.len() as u32)
        .ok_or(UnitError::Overflow)?;
    let numerator = whole
        .checked_mul(denominator)
        .and_then(|n| n.checked_add(frac))
        .ok_or(UnitError::Overflow)?;
    Ok((sign * numerator, denominator))
}

fn convert(numerator: i128, denominator: i128, unit: Unit) -> Result<Conversion, UnitError> {
    let scaled = numerator
        .checked_mul(unit.micrometres())
        .ok_or(UnitError::Overflow)?;
    let quotient = scaled / denominator;
    let remainder = scaled % denominator;
    let rounded = if remainder.unsigned_abs() * 2 >= denominator as u128 {
        quotient
            .checked_add(scaled.signum())
            .ok_or(UnitError::Overflow)?
    } else {
        quotient
    };
    let result = Length(i64::try_from(rounded).map_err(|_| UnitError::Overflow)?);
    Ok(if remainder == 0 {
        Conversion::Exact(result)
    } else {
        Conversion::NeedsConfirmation(result)
    })
}

/// Rejects a zero/negative proposed dimension, including a rounded-to-zero suggestion.
pub fn dimension(proposal: Conversion) -> Result<Conversion, UnitError> {
    proposal.suggested().positive()?;
    Ok(proposal)
}

pub const WORLD_LIMIT_MM: f64 = 1_000_000.0;

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Quaternion {
    /// Scalar first, then the X/Y/Z vector components.
    pub w: f64,
    pub x: f64,
    pub y: f64,
    pub z: f64,
}

impl Quaternion {
    pub const IDENTITY: Self = Self {
        w: 1.0,
        x: 0.0,
        y: 0.0,
        z: 0.0,
    };

    pub fn normalized(w: f64, x: f64, y: f64, z: f64) -> Result<Self, UnitError> {
        let parts = [w, x, y, z];
        if !parts.iter().all(|v| v.is_finite()) {
            return Err(UnitError::NonFinite);
        }
        let scale = parts.iter().fold(0.0_f64, |a, b| a.max(b.abs()));
        if scale == 0.0 {
            return Err(UnitError::InvalidRotation);
        }
        let parts = parts.map(|v| v / scale);
        let norm = parts.iter().map(|v| v * v).sum::<f64>().sqrt();
        Ok(Self {
            w: parts[0] / norm,
            x: parts[1] / norm,
            y: parts[2] / norm,
            z: parts[3] / norm,
        })
    }

    pub fn rotate(self, v: [f64; 3]) -> [f64; 3] {
        let q = [self.x, self.y, self.z];
        let cross = |a: [f64; 3], b: [f64; 3]| {
            [
                a[1] * b[2] - a[2] * b[1],
                a[2] * b[0] - a[0] * b[2],
                a[0] * b[1] - a[1] * b[0],
            ]
        };
        let t = cross(q, v).map(|c| 2.0 * c);
        let qt = cross(q, t);
        std::array::from_fn(|i| v[i] + self.w * t[i] + qt[i])
    }

    pub fn compose(self, other: Self) -> Result<Self, UnitError> {
        Self::normalized(
            self.w * other.w - self.x * other.x - self.y * other.y - self.z * other.z,
            self.w * other.x + self.x * other.w + self.y * other.z - self.z * other.y,
            self.w * other.y - self.x * other.z + self.y * other.w + self.z * other.x,
            self.w * other.z + self.x * other.y - self.y * other.x + self.z * other.w,
        )
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Pose {
    pub translation_mm: [f64; 3],
    pub rotation: Quaternion,
}

impl Pose {
    /// No translation and no rotation: the world frame.
    pub const IDENTITY: Self = Self {
        translation_mm: [0.0; 3],
        rotation: Quaternion::IDENTITY,
    };

    pub fn new(translation_mm: [f64; 3], rotation: Quaternion) -> Result<Self, UnitError> {
        validate_position(translation_mm)?;
        let rotation = Quaternion::normalized(rotation.w, rotation.x, rotation.y, rotation.z)?;
        Ok(Self {
            translation_mm,
            rotation,
        })
    }

    /// Explicit numeric positions use the manufacturing grid, unlike derived poses.
    pub fn from_explicit(position: [Length; 3], rotation: Quaternion) -> Result<Self, UnitError> {
        Self::new(position.map(|v| v.0 as f64 / 1000.0), rotation)
    }

    pub fn transform_point(self, local_mm: [f64; 3]) -> Result<[f64; 3], UnitError> {
        let rotated = self.rotation.rotate(local_mm);
        let point = std::array::from_fn(|i| self.translation_mm[i] + rotated[i]);
        validate_position(point)?;
        Ok(point)
    }

    pub fn compose(self, child: Self) -> Result<Self, UnitError> {
        Self::new(
            self.transform_point(child.translation_mm)?,
            self.rotation.compose(child.rotation)?,
        )
    }

    /// Preserve a local minimum face, midpoint or maximum face while resizing.
    /// Checks both the resulting origin and anchored face against world bounds.
    pub fn resized(
        self,
        axis: usize,
        old: Length,
        new: Length,
        anchor: Anchor,
    ) -> Result<Self, UnitError> {
        if axis >= 3 {
            return Err(UnitError::OutOfBounds);
        }
        old.positive()?;
        new.positive()?;
        let delta = (new.0 as i128 - old.0 as i128) as f64 / 1000.0;
        let shift = match anchor {
            Anchor::Start => 0.0,
            Anchor::Centre => -delta / 2.0,
            Anchor::End => -delta,
        };
        let mut local = [0.0; 3];
        local[axis] = shift;
        let result = Self::new(self.transform_point(local)?, self.rotation)?;
        local[axis] = new.0 as f64 / 1000.0;
        result.transform_point(local)?;
        Ok(result)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Centre,
    End,
}

fn validate_position(position: [f64; 3]) -> Result<(), UnitError> {
    for coordinate in position {
        if !coordinate.is_finite() {
            return Err(UnitError::NonFinite);
        }
        if coordinate.abs() > WORLD_LIMIT_MM {
            return Err(UnitError::OutOfBounds);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_units_and_fractional_inches() {
        for (text, unit, expected) in [
            ("12.345", Unit::Mm, 12345),
            ("1,234", Unit::Cm, 12340),
            ("1", Unit::M, 1000000),
            ("1.5", Unit::Inch, 38100),
            ("2", Unit::Foot, 609600),
        ] {
            assert_eq!(
                Length::from_decimal(text, unit),
                Ok(Conversion::Exact(Length(expected)))
            );
        }
        assert_eq!(
            Length::from_inch_fraction("1 1/2"),
            Ok(Conversion::Exact(Length(38100)))
        );
        assert_eq!(
            Length::from_inch_fraction("3/4"),
            Ok(Conversion::Exact(Length(19050)))
        );
    }

    #[test]
    fn rounding_is_a_proposal_and_cannot_erase_a_dimension() {
        assert_eq!(
            dimension(Length::from_decimal("12.3456", Unit::Mm).unwrap()),
            Ok(Conversion::NeedsConfirmation(Length(12346)))
        );
        assert_eq!(
            Length::from_inch_fraction("1/64"),
            Ok(Conversion::NeedsConfirmation(Length(397)))
        );
        assert_eq!(
            dimension(Length::from_decimal("0.0004", Unit::Mm).unwrap()),
            Err(UnitError::NonPositiveDimension)
        );
        assert_eq!(
            Length::from_decimal("-0.0005", Unit::Mm),
            Ok(Conversion::NeedsConfirmation(Length(-1)))
        );
    }

    #[test]
    fn rejects_invalid_and_overflowing_inputs() {
        for invalid in ["", "1.2.3", "1,234.5", "1 000", "NaN", "1e3"] {
            assert!(Length::from_decimal(invalid, Unit::Mm).is_err());
        }
        for invalid in ["3/0", "2/2", "1 3/2", "1//2"] {
            assert!(Length::from_inch_fraction(invalid).is_err());
        }
        assert_eq!(
            dimension(Conversion::Exact(Length::ZERO)),
            Err(UnitError::NonPositiveDimension)
        );
        assert_eq!(
            dimension(Conversion::Exact(Length(-1))),
            Err(UnitError::NonPositiveDimension)
        );
        assert_eq!(
            Length::from_decimal("9223372036854776", Unit::Mm),
            Err(UnitError::Overflow)
        );
        assert_eq!(
            Length(i64::MAX).checked_add(Length(1)),
            Err(UnitError::Overflow)
        );
        assert_eq!(
            Length(i64::MIN).checked_sub(Length(1)),
            Err(UnitError::Overflow)
        );
    }

    #[test]
    fn bounded_positions_and_normalized_rotations() {
        let q = Quaternion::normalized(2.0, 0.0, 0.0, 2.0).unwrap();
        assert!((q.w * q.w + q.z * q.z - 1.0).abs() < 1e-15);
        assert_eq!(
            Quaternion::normalized(0.0, 0.0, 0.0, 0.0),
            Err(UnitError::InvalidRotation)
        );
        assert_eq!(
            Pose::new([f64::NAN, 0.0, 0.0], q),
            Err(UnitError::NonFinite)
        );
        assert_eq!(
            Pose::new([1_000_000.000001, 0.0, 0.0], q),
            Err(UnitError::OutOfBounds)
        );
        assert!(
            Pose::from_explicit([Length(1_000_000_000), Length::ZERO, Length::ZERO], q).is_ok()
        );
        assert_eq!(
            Pose::from_explicit([Length(1_000_000_001), Length::ZERO, Length::ZERO], q),
            Err(UnitError::OutOfBounds)
        );
        assert_eq!(
            Quaternion::normalized(f64::INFINITY, 0.0, 0.0, 0.0),
            Err(UnitError::NonFinite)
        );
        let edge = Pose::new([999_999.999, 0.0, 0.0], Quaternion::IDENTITY).unwrap();
        assert_eq!(
            edge.resized(0, Length(1), Length(2), Anchor::Start),
            Err(UnitError::OutOfBounds)
        );
    }

    #[test]
    fn centred_half_grid_resize_in_rotated_parent_keeps_world_centre() {
        let quarter_turn = Quaternion::normalized(1.0, 0.0, 0.0, 1.0).unwrap();
        let parent = Pose::new([100.0, 200.0, 0.0], quarter_turn).unwrap();
        let before = Pose::new([10.0, 20.0, 0.0], Quaternion::IDENTITY).unwrap();
        let old = Length(100_000);
        let new = Length(100_001);
        let after = before.resized(0, old, new, Anchor::Centre).unwrap();
        assert!((after.translation_mm[0] - 9.9995).abs() < 1e-12);
        let old_centre = parent
            .compose(before)
            .unwrap()
            .transform_point([50.0, 0.0, 0.0])
            .unwrap();
        let new_centre = parent
            .compose(after)
            .unwrap()
            .transform_point([50.0005, 0.0, 0.0])
            .unwrap();
        for i in 0..3 {
            assert!((old_centre[i] - new_centre[i]).abs() <= 1e-6);
        }
        let end = before.resized(0, old, new, Anchor::End).unwrap();
        assert!((end.transform_point([100.001, 0.0, 0.0]).unwrap()[0] - 110.0).abs() < 1e-9);
        let thickness = before
            .resized(2, Length(18_000), Length(15_000), Anchor::Centre)
            .unwrap();
        assert!((thickness.translation_mm[2] - 1.5).abs() < 1e-12);
        assert!((thickness.transform_point([0.0, 0.0, 7.5]).unwrap()[2] - 9.0).abs() < 1e-12);
    }
}
