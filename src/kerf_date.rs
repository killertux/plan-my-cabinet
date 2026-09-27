//! Presentation of recorded shop confirmation time; never infer a date on load.

/// A four-digit UTC calendar date for a recorded Unix-millisecond timestamp.
/// `None` means an older confirmed document did not record when confirmation
/// occurred. A malformed timestamp is never printed as a believable date.
pub fn confirmation_date_utc(timestamp_ms: Option<u64>) -> Option<String> {
    let ms = timestamp_ms?;
    let nanos = i128::from(ms).checked_mul(1_000_000)?;
    let date = time::OffsetDateTime::from_unix_timestamp_nanos(nanos)
        .ok()?
        .date();
    if !(1970..=9999).contains(&date.year()) {
        return None;
    }
    Some(format!(
        "{:04}-{:02}-{:02} UTC",
        date.year(),
        date.month() as u8,
        date.day()
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn utc_date_is_real_and_never_inferred() {
        assert_eq!(confirmation_date_utc(None), None);
        assert_eq!(
            confirmation_date_utc(Some(1_780_000_000_000)),
            Some("2026-05-28 UTC".into())
        );
        assert_eq!(
            confirmation_date_utc(Some(1_709_251_199_999)),
            Some("2024-02-29 UTC".into())
        );
        assert_eq!(confirmation_date_utc(Some(u64::MAX)), None);
    }
}
