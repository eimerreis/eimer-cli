//! Durations in hours. codeunity time stores them as hours * 100.

use anyhow::{Result, bail};

const MAX_HOURS: f64 = 24.0;

/// Parses "1.5", "1,5h", "90m", "1h30m" or "1:30" into hours.
pub fn parse(input: &str) -> Result<f64> {
    let value = input.trim().to_lowercase().replace(',', ".");
    let hours = parse_hours(&value).filter(|h| h.is_finite() && *h > 0.0 && *h <= MAX_HOURS);
    match hours {
        Some(h) => Ok((h * 100.0).round() / 100.0),
        None => bail!(
            "Invalid duration \"{input}\". Use e.g. 1.5h, 90m, 1h30m or 1:30 (max {MAX_HOURS}h)."
        ),
    }
}

fn parse_hours(value: &str) -> Option<f64> {
    let number = |s: &str| -> Option<f64> {
        let valid = !s.is_empty()
            && s.chars().all(|c| c.is_ascii_digit() || c == '.')
            && s.matches('.').count() <= 1
            && !s.starts_with('.')
            && !s.ends_with('.');
        valid.then(|| s.parse().ok()).flatten()
    };
    let integer = |s: &str| -> Option<f64> {
        (!s.is_empty() && s.chars().all(|c| c.is_ascii_digit())).then(|| s.parse().ok())?
    };

    if let Some((h, m)) = value.split_once(':') {
        if m.len() != 2 {
            return None;
        }
        let minutes = integer(m).filter(|m| *m < 60.0)?;
        return Some(integer(h)? + minutes / 60.0);
    }
    if let Some(h) = number(value) {
        return Some(h);
    }
    let (hours, rest) = match value.split_once('h') {
        Some((h, rest)) => (Some(number(h)?), rest.trim()),
        None => (None, value),
    };
    let minutes = match rest {
        "" => None,
        m => Some(integer(m.strip_suffix('m')?)?),
    };
    if hours.is_none() && minutes.is_none() {
        return None;
    }
    Some(hours.unwrap_or(0.0) + minutes.unwrap_or(0.0) / 60.0)
}

/// "1.5h", "2h", "0.33h".
pub fn format(hours: f64) -> String {
    // `+ 0.0` turns -0.0 (the sum of no values) into 0.0.
    let rounded = (hours * 100.0).round() / 100.0 + 0.0;
    let text = format!("{rounded:.2}");
    let text = text.trim_end_matches('0').trim_end_matches('.');
    format!("{text}h")
}

pub fn to_api(hours: f64) -> i64 {
    (hours * 100.0).round() as i64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_supported_formats() {
        for input in ["1.5", "1,5h", "90m", "1h30m", "1h 30m", "1:30"] {
            assert_eq!(parse(input).unwrap(), 1.5, "{input}");
        }
        assert_eq!(parse("2h").unwrap(), 2.0);
        assert_eq!(parse("20m").unwrap(), 0.33);
    }

    #[test]
    fn rejects_invalid_durations() {
        for input in [
            "", "0", "abc", "25h", "1:75", "1:5", "-1", "h", "m", "1.5.2", "1x",
        ] {
            assert!(parse(input).is_err(), "{input}");
        }
    }

    #[test]
    fn formats_and_converts() {
        assert_eq!(format(1.5), "1.5h");
        assert_eq!(format(2.0), "2h");
        assert_eq!(format(std::iter::empty::<f64>().sum()), "0h");
        assert_eq!(format(0.333), "0.33h");
        assert_eq!(to_api(1.5), 150);
    }
}
