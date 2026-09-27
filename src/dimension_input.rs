//! UI-independent dimension text, presentation and transient edit state.
//! Editable text is deliberately ungrouped in every locale.

use crate::units::{Conversion, Length, Unit, UnitError, dimension};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Locale {
    En,
    PtBr,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum InputError {
    /// Mixed/repeated decimal marks or whitespace-separated/grouped digits.
    GroupingSeparators,
    InvalidNumber,
    InvalidFraction,
    FractionRequiresInches,
    Unit(UnitError),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ParsedDimension {
    pub unit: Unit,
    pub conversion: Conversion,
}

/// Parse a length without imposing a positivity constraint (signed positions use this too).
/// A suffix overrides the field unit. A slash is allowed only for inch fractions.
pub fn parse_length(text: &str, field_unit: Unit) -> Result<ParsedDimension, InputError> {
    let text = text.trim();
    let (number, unit) = suffix(text).unwrap_or((text, field_unit));
    let number = number.trim_end();
    let conversion = if number.contains('/') {
        if unit != Unit::Inch {
            return Err(InputError::FractionRequiresInches);
        }
        // Only one ordinary space may separate a whole number and a fraction.
        let unsigned = number.trim_start_matches(['+', '-']);
        let shape = if let Some((whole, fraction)) = unsigned.split_once(' ') {
            !whole.is_empty()
                && whole.bytes().all(|b| b.is_ascii_digit())
                && !fraction.contains(' ')
                && fraction.contains('/')
        } else {
            true
        };
        if !shape || number.contains(['\t', '\n', '\r']) {
            return Err(InputError::InvalidFraction);
        }
        Length::from_inch_fraction(number).map_err(|error| match error {
            UnitError::InvalidFraction => InputError::InvalidFraction,
            other => InputError::Unit(other),
        })?
    } else {
        if number.contains(char::is_whitespace) || number.matches(['.', ',']).count() > 1 {
            return Err(InputError::GroupingSeparators);
        }
        Length::from_decimal(number, unit).map_err(|error| match error {
            UnitError::InvalidNumber => InputError::InvalidNumber,
            other => InputError::Unit(other),
        })?
    };
    Ok(ParsedDimension { unit, conversion })
}

fn suffix(text: &str) -> Option<(&str, Unit)> {
    // Longest names first; symbols have their own exact suffixes.
    for (name, unit) in [
        ("mm", Unit::Mm),
        ("cm", Unit::Cm),
        ("ft", Unit::Foot),
        ("in", Unit::Inch),
        ("m", Unit::M),
    ] {
        if let Some(head) = text.get(..text.len().saturating_sub(name.len())) {
            let tail = &text[head.len()..];
            if tail.eq_ignore_ascii_case(name) {
                return Some((head, unit));
            }
        }
    }
    for (mark, unit) in [
        ("\"", Unit::Inch),
        ("'", Unit::Foot),
        ("″", Unit::Inch),
        ("′", Unit::Foot),
    ] {
        if let Some(head) = text.strip_suffix(mark) {
            return Some((head, unit));
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn exact(text: &str, field: Unit, expected: i64) {
        for locale in [Locale::En, Locale::PtBr] {
            let mut input = DimensionInput::new(Length::from_micrometres(1), field, locale, 2);
            assert_eq!(
                input.edit(text).as_ref().unwrap().conversion,
                Conversion::Exact(Length::from_micrometres(expected)),
                "{text}"
            );
            assert_eq!(input.commit(false), Ok(Length::from_micrometres(expected)));
        }
    }

    #[test]
    fn comma_dot_suffixes_and_field_unit_are_locale_independent() {
        exact("1,234 mm", Unit::Inch, 1234);
        exact("1.234 mm", Unit::Foot, 1234);
        exact("1,234", Unit::Mm, 1234);
        for (text, expected) in [
            ("2 cm", 20_000),
            ("0.2 m", 200_000),
            ("1 in", 25_400),
            ("1 ft", 304_800),
            ("1\"", 25_400),
            ("1'", 304_800),
            ("1″", 25_400),
            ("1′", 304_800),
            ("1.5 IN", 38_100),
        ] {
            exact(text, Unit::Mm, expected);
        }
        exact("1", Unit::Cm, 10_000);
        exact("3/4 in", Unit::Mm, 19_050);
        exact("1 1/2 in", Unit::Mm, 38_100);
        exact("1 1/2", Unit::Inch, 38_100);
    }

    #[test]
    fn rejects_grouping_ambiguity_and_invalid_fractions_without_commit() {
        let original = Length::from_micrometres(12_345);
        let mut input = DimensionInput::new(original, Unit::Mm, Locale::En, 2);
        for text in [
            "1,234.5 mm",
            "1.234,5 mm",
            "1..2",
            "1,,2",
            "1 000 mm",
            "1\t000 mm",
        ] {
            assert_eq!(
                input.edit(text),
                &Err(InputError::GroupingSeparators),
                "{text}"
            );
            assert!(matches!(input.commit(true), Err(CommitError::Invalid(_))));
            assert_eq!(input.committed(), original);
        }
        for text in [
            "3/0 in",
            "2/2 in",
            "1 3/2 in",
            "1//2 in",
            "1  1/2 in",
            "1/2/3 in",
        ] {
            assert_eq!(
                input.edit(text),
                &Err(InputError::InvalidFraction),
                "{text}"
            );
            assert_eq!(input.committed(), original);
        }
        assert_eq!(
            input.edit("1/2 cm"),
            &Err(InputError::FractionRequiresInches)
        );
        for text in ["", "NaN", "1e3", "1.", ".5", "12 mm junk"] {
            assert!(input.edit(text).is_err(), "{text}");
        }
    }

    #[test]
    fn display_rounding_focus_blur_and_locale_switch_do_not_change_storage() {
        let physical = Length::from_micrometres(12_345);
        let mut input = DimensionInput::new(physical, Unit::Mm, Locale::En, 2);
        assert_eq!(input.display(), "12.35 mm");
        assert_eq!(input.focus(), "12.35 mm");
        input.blur();
        assert_eq!(input.committed(), physical);
        assert_eq!(input.commit(false), Err(CommitError::NoEdit));
        input.set_presentation(Unit::Mm, Locale::PtBr);
        assert_eq!(input.focus(), "12,35 mm");
        input.set_presentation(Unit::Inch, Locale::PtBr);
        assert_eq!(input.display(), "0,49 in");
        input.set_presentation(Unit::Mm, Locale::En);
        assert_eq!(input.committed(), physical);
        assert_eq!(input.display(), "12.35 mm");
    }

    #[test]
    fn rounding_requires_consent_and_cancellation_preserves_value() {
        let old = Length::from_micrometres(12_345);
        let mut input = DimensionInput::new(old, Unit::Mm, Locale::En, 2);
        assert_eq!(
            input.edit("1/64 in").as_ref().unwrap().conversion,
            Conversion::NeedsConfirmation(Length::from_micrometres(397))
        );
        assert_eq!(
            input.rounding_preview(),
            Some(("1/64 in".into(), "0.397 mm".into()))
        );
        input.blur();
        assert_eq!(
            input.commit(false),
            Err(CommitError::NeedsConfirmation(Length::from_micrometres(
                397
            )))
        );
        assert_eq!(input.committed(), old);
        input.cancel();
        assert_eq!(input.committed(), old);
        input.edit("12.3456 mm");
        assert_eq!(
            input.rounding_preview(),
            Some(("12.3456 mm".into(), "12.346 mm".into()))
        );
        assert_eq!(input.commit(true), Ok(Length::from_micrometres(12_346)));
        assert_eq!(
            input.edit("0.0004 mm"),
            &Err(InputError::Unit(UnitError::NonPositiveDimension))
        );
        assert_eq!(
            input.edit("-1 mm"),
            &Err(InputError::Unit(UnitError::NonPositiveDimension))
        );
        assert_eq!(input.committed(), Length::from_micrometres(12_346));
    }

    #[test]
    fn locale_change_preserves_pending_proposal_and_large_values_format_safely() {
        let old = Length::from_micrometres(1_000);
        let mut input = DimensionInput::new(old, Unit::Mm, Locale::En, 2);
        input.edit("1/64 in");
        input.set_presentation(Unit::Foot, Locale::PtBr);
        assert_eq!(input.focus(), "1/64 in");
        assert_eq!(
            input.rounding_preview(),
            Some(("1/64 in".into(), "0.397 mm".into()))
        );
        assert_eq!(input.committed(), old);
        assert_eq!(input.commit(true), Ok(Length::from_micrometres(397)));
        assert_eq!(input.display(), "0,00 ft");
        assert_eq!(
            format_length(Length::from_micrometres(i64::MIN), Unit::Mm, Locale::En, 9),
            "-9223372036854775.808000000 mm"
        );
        assert_eq!(
            parse_length("999999999999999999999999 mm", Unit::Mm),
            Err(InputError::Unit(UnitError::Overflow))
        );
    }

    #[test]
    fn unsuffixed_draft_keeps_entry_unit_through_preferences_and_further_edits() {
        let old = Length::from_micrometres(100_000);
        let mut input = DimensionInput::new(old, Unit::Mm, Locale::PtBr, 2);
        assert_eq!(input.edit("1,5").as_ref().unwrap().unit, Unit::Mm);
        input.set_presentation(Unit::Cm, Locale::En);
        assert_eq!(input.focus(), "1,5");
        assert_eq!(input.commit(false), Ok(Length::from_micrometres(1_500)));
        assert_eq!(input.display(), "0.15 cm");
        assert_eq!(input.edit("2").as_ref().unwrap().unit, Unit::Cm);
        input.set_presentation(Unit::M, Locale::PtBr);
        assert_eq!(input.edit("3").as_ref().unwrap().unit, Unit::Cm);
        assert_eq!(input.commit(false), Ok(Length::from_micrometres(30_000)));
        assert_eq!(input.display(), "0,03 m");
    }

    #[test]
    fn explicit_suffix_and_rounding_consent_survive_presentation_changes() {
        let original = Length::from_micrometres(12_345);
        let mut input = DimensionInput::new(original, Unit::Cm, Locale::PtBr, 2);
        input.edit("1/64 in");
        assert_eq!(
            input.rounding_preview(),
            Some(("1/64 in".into(), "0,397 mm".into()))
        );
        input.set_presentation(Unit::Foot, Locale::En);
        assert_eq!(input.focus(), "1/64 in");
        assert_eq!(input.committed(), original);
        assert_eq!(
            input.commit(false),
            Err(CommitError::NeedsConfirmation(Length::from_micrometres(
                397
            )))
        );
        assert_eq!(input.commit(true), Ok(Length::from_micrometres(397)));

        // An untouched field may reformat in the new unit without becoming an edit.
        let mut pristine = DimensionInput::new(original, Unit::Mm, Locale::En, 2);
        pristine.set_presentation(Unit::Cm, Locale::PtBr);
        assert_eq!(pristine.committed(), original);
        assert_eq!(pristine.commit(false), Err(CommitError::NoEdit));
    }

    #[test]
    fn invalid_draft_stays_invalid_and_cancel_uses_latest_presentation() {
        let old = Length::from_micrometres(100_000);
        let mut input = DimensionInput::new(old, Unit::Mm, Locale::PtBr, 2);
        assert_eq!(input.edit("1/2"), &Err(InputError::FractionRequiresInches));
        input.set_presentation(Unit::Inch, Locale::En);
        assert_eq!(
            input.preview(),
            Some(&Err(InputError::FractionRequiresInches))
        );
        assert_eq!(input.edit("1/4"), &Err(InputError::FractionRequiresInches));
        input.cancel();
        assert_eq!(input.focus(), input.display());
        assert_eq!(input.edit("1/4").as_ref().unwrap().unit, Unit::Inch);
        assert_eq!(input.committed(), old);
    }
}

fn unit_name(unit: Unit) -> &'static str {
    match unit {
        Unit::Mm => "mm",
        Unit::Cm => "cm",
        Unit::M => "m",
        Unit::Inch => "in",
        Unit::Foot => "ft",
    }
}

fn micrometres_per_unit(unit: Unit) -> i128 {
    match unit {
        Unit::Mm => 1_000,
        Unit::Cm => 10_000,
        Unit::M => 1_000_000,
        Unit::Inch => 25_400,
        Unit::Foot => 304_800,
    }
}

/// Round for presentation only, half away from zero; never use this text as a commit.
/// `decimals` is limited to nine, well beyond the manufacturing grid in all units.
pub fn format_length(value: Length, unit: Unit, locale: Locale, decimals: u8) -> String {
    assert!(
        decimals <= 9,
        "display precision must be at most nine decimals"
    );
    let scale = 10_i128.pow(u32::from(decimals));
    let magnitude = i128::from(value.micrometres()).abs() * scale;
    let divisor = micrometres_per_unit(unit);
    let rounded = (magnitude + divisor / 2) / divisor;
    let sign = if value.micrometres() < 0 && rounded != 0 {
        "-"
    } else {
        ""
    };
    if decimals == 0 {
        return format!("{sign}{rounded} {}", unit_name(unit));
    }
    let separator = match locale {
        Locale::En => '.',
        Locale::PtBr => ',',
    };
    format!(
        "{sign}{}{separator}{:0width$} {}",
        rounded / scale,
        rounded % scale,
        unit_name(unit),
        width = usize::from(decimals)
    )
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CommitError {
    NoEdit,
    Invalid(InputError),
    /// The suggested grid value needs an explicit subsequent confirmed commit.
    NeedsConfirmation(Length),
}

/// Owns a field's uncommitted text independently of its canonical length.
pub struct DimensionInput {
    committed: Length,
    field_unit: Unit,
    locale: Locale,
    decimals: u8,
    draft: Option<String>,
    preview: Option<Result<ParsedDimension, InputError>>,
    /// The unit and input locale when this edit started; presentation may change independently.
    entry: Option<(Unit, Locale)>,
}

impl DimensionInput {
    pub fn new(committed: Length, field_unit: Unit, locale: Locale, decimals: u8) -> Self {
        assert!(decimals <= 9);
        Self {
            committed,
            field_unit,
            locale,
            decimals,
            draft: None,
            preview: None,
            entry: None,
        }
    }

    pub fn committed(&self) -> Length {
        self.committed
    }

    /// Focus merely obtains display text. Blur does not parse or commit it.
    pub fn focus(&self) -> String {
        self.draft.clone().unwrap_or_else(|| self.display())
    }

    pub fn blur(&self) {}

    pub fn display(&self) -> String {
        format_length(self.committed, self.field_unit, self.locale, self.decimals)
    }

    pub fn set_presentation(&mut self, unit: Unit, locale: Locale) {
        self.field_unit = unit;
        self.locale = locale;
        // Retain the entry semantics as well as the already-parsed proposal.
    }

    pub fn edit(&mut self, text: impl Into<String>) -> &Result<ParsedDimension, InputError> {
        let (entry_unit, _) = *self.entry.get_or_insert((self.field_unit, self.locale));
        self.draft = Some(text.into());
        self.preview = Some(
            parse_length(self.draft.as_deref().unwrap(), entry_unit).and_then(|parsed| {
                dimension(parsed.conversion).map_err(InputError::Unit)?;
                Ok(parsed)
            }),
        );
        self.preview.as_ref().unwrap()
    }

    pub fn preview(&self) -> Option<&Result<ParsedDimension, InputError>> {
        self.preview.as_ref()
    }

    /// Show the entered text and a full-grid-precision suggestion before consent.
    pub fn rounding_preview(&self) -> Option<(String, String)> {
        match self.preview.as_ref()? {
            Ok(ParsedDimension {
                conversion: Conversion::NeedsConfirmation(value),
                ..
            }) => Some((
                self.draft.clone()?,
                format_length(
                    *value,
                    Unit::Mm,
                    self.entry.map_or(self.locale, |(_, locale)| locale),
                    3,
                ),
            )),
            _ => None,
        }
    }

    pub fn commit(&mut self, confirm_rounding: bool) -> Result<Length, CommitError> {
        let parsed = *self
            .preview
            .as_ref()
            .ok_or(CommitError::NoEdit)?
            .as_ref()
            .map_err(|error| CommitError::Invalid(*error))?;
        let value = match parsed.conversion {
            Conversion::Exact(value) => value,
            Conversion::NeedsConfirmation(value) if confirm_rounding => value,
            Conversion::NeedsConfirmation(value) => {
                return Err(CommitError::NeedsConfirmation(value));
            }
        };
        self.committed = value;
        self.cancel();
        Ok(value)
    }

    pub fn cancel(&mut self) {
        self.draft = None;
        self.preview = None;
        self.entry = None;
    }
}
