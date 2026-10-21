use anyhow::{Context, Result, bail};

/// Converts a UTC RFC 3339 timestamp such as GitHub's `2026-10-15T12:34:56Z` to Unix seconds.
pub(crate) fn parse_utc(timestamp: &str) -> Result<u64> {
    let invalid =
        || format!("expected a UTC timestamp like 2026-10-15T12:34:56Z, found {timestamp}");
    let Some(body) = timestamp.strip_suffix('Z') else {
        bail!(invalid());
    };
    let (date, time) = body.split_once('T').with_context(invalid)?;
    let [year, month, day] = fields(date, '-').with_context(invalid)?;
    let [hour, minute, second] = fields(time, ':').with_context(invalid)?;
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 60
    {
        bail!(invalid());
    }

    let days = days_from_civil(year, month, day);
    let seconds = days * 86_400 + i64::from(hour * 3_600 + minute * 60 + second);
    u64::try_from(seconds).with_context(invalid)
}

fn fields(text: &str, separator: char) -> Option<[u32; 3]> {
    let mut parts = text.split(separator).map(|part| part.parse::<u32>().ok());
    let fields = [parts.next()??, parts.next()??, parts.next()??];
    parts.next().is_none().then_some(fields)
}

/// Days since 1970-01-01 in the proleptic Gregorian calendar (Howard Hinnant's algorithm).
fn days_from_civil(year: u32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let year_of_era = year - era * 400;
    let month = i64::from(month);
    let day_of_year = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_utc_converts_github_timestamps() -> Result<()> {
        assert_eq!(parse_utc("1970-01-01T00:00:00Z")?, 0);
        assert_eq!(parse_utc("2000-03-01T00:00:00Z")?, 951_868_800);
        assert_eq!(parse_utc("2026-10-15T12:34:56Z")?, 1_792_067_696);
        Ok(())
    }

    #[test]
    fn parse_utc_rejects_offsets_and_malformed_values() {
        let malformed = [
            "2026-10-15T12:34:56+09:00",
            "2026-10-15 12:34:56Z",
            "2026-13-15T12:34:56Z",
            "2026-10-15T12:34Z",
        ];
        let accepted: Vec<_> = malformed
            .into_iter()
            .filter(|timestamp| parse_utc(timestamp).is_ok())
            .collect();
        assert!(accepted.is_empty(), "{accepted:?} should be rejected");
    }
}
