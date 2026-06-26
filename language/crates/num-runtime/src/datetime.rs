#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct UtcDateTime {
    year: i32,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
    second: u32,
}

pub fn parse_iso_utc(value: &str) -> Result<String, String> {
    parse_iso_utc_parts(value).map(format_iso_utc_parts)
}

pub fn format_iso_utc(value: &str) -> Result<String, String> {
    parse_iso_utc(value)
}

pub fn parse_duration_hours(value: &str) -> Result<f64, String> {
    let normalized = value.trim().replace(' ', "");
    let Some(number) = normalized.strip_suffix('h') else {
        return Err("expected an hour duration ending in `h`".to_string());
    };
    let hours = number
        .parse::<f64>()
        .map_err(|_| "expected a numeric hour amount before `h`".to_string())?;
    if !hours.is_finite() {
        return Err("duration hours must be finite".to_string());
    }
    Ok(hours)
}

pub fn format_duration_hours(hours: f64) -> Result<String, String> {
    if !hours.is_finite() {
        return Err("duration hours must be finite".to_string());
    }
    if hours.fract().abs() < f64::EPSILON {
        Ok(format!("{}h", hours as i64))
    } else {
        Ok(format!("{hours}h"))
    }
}

pub fn add_duration_hours(value: &str, hours: f64) -> Result<String, String> {
    shift_duration_hours(value, hours)
}

pub fn subtract_duration_hours(value: &str, hours: f64) -> Result<String, String> {
    shift_duration_hours(value, -hours)
}

pub fn compare_iso_utc(left: &str, right: &str) -> Result<std::cmp::Ordering, String> {
    let left = timestamp_seconds(parse_iso_utc_parts(left)?)?;
    let right = timestamp_seconds(parse_iso_utc_parts(right)?)?;
    Ok(left.cmp(&right))
}

fn shift_duration_hours(value: &str, hours: f64) -> Result<String, String> {
    if !hours.is_finite() {
        return Err("duration hours must be finite".to_string());
    }
    let seconds_delta = hours * 3600.0;
    if seconds_delta.fract().abs() > f64::EPSILON {
        return Err("DateTime arithmetic requires whole-second hour durations".to_string());
    }
    let base = timestamp_seconds(parse_iso_utc_parts(value)?)?;
    let shifted = base
        .checked_add(seconds_delta as i64)
        .ok_or_else(|| "DateTime arithmetic overflowed".to_string())?;
    Ok(format_iso_utc_parts(datetime_from_timestamp_seconds(
        shifted,
    )))
}

fn parse_iso_utc_parts(value: &str) -> Result<UtcDateTime, String> {
    let bytes = value.as_bytes();
    if bytes.len() != 20
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes[10] != b'T'
        || bytes[13] != b':'
        || bytes[16] != b':'
        || bytes[19] != b'Z'
    {
        return Err("expected UTC ISO-8601 form YYYY-MM-DDTHH:MM:SSZ".to_string());
    }
    let parsed = UtcDateTime {
        year: parse_fixed_i32(value, 0, 4, "year")?,
        month: parse_fixed_u32(value, 5, 7, "month")?,
        day: parse_fixed_u32(value, 8, 10, "day")?,
        hour: parse_fixed_u32(value, 11, 13, "hour")?,
        minute: parse_fixed_u32(value, 14, 16, "minute")?,
        second: parse_fixed_u32(value, 17, 19, "second")?,
    };
    if !(1..=12).contains(&parsed.month) {
        return Err("month must be 01..12".to_string());
    }
    if parsed.day == 0 || parsed.day > days_in_month(parsed.year, parsed.month) {
        return Err("day is outside the month range".to_string());
    }
    if parsed.hour > 23 || parsed.minute > 59 || parsed.second > 59 {
        return Err("time must be within 00:00:00..23:59:59".to_string());
    }
    Ok(parsed)
}

fn format_iso_utc_parts(value: UtcDateTime) -> String {
    format!(
        "{:04}-{:02}-{:02}T{:02}:{:02}:{:02}Z",
        value.year, value.month, value.day, value.hour, value.minute, value.second
    )
}

fn parse_fixed_i32(value: &str, start: usize, end: usize, label: &str) -> Result<i32, String> {
    value[start..end]
        .parse::<i32>()
        .map_err(|_| format!("{label} must be numeric"))
}

fn parse_fixed_u32(value: &str, start: usize, end: usize, label: &str) -> Result<u32, String> {
    value[start..end]
        .parse::<u32>()
        .map_err(|_| format!("{label} must be numeric"))
}

fn timestamp_seconds(value: UtcDateTime) -> Result<i64, String> {
    let days = days_from_civil(value.year, value.month, value.day);
    let day_seconds =
        i64::from(value.hour) * 3600 + i64::from(value.minute) * 60 + i64::from(value.second);
    days.checked_mul(86_400)
        .and_then(|seconds| seconds.checked_add(day_seconds))
        .ok_or_else(|| "DateTime timestamp overflowed".to_string())
}

fn datetime_from_timestamp_seconds(seconds: i64) -> UtcDateTime {
    let days = seconds.div_euclid(86_400);
    let day_seconds = seconds.rem_euclid(86_400);
    let (year, month, day) = civil_from_days(days);
    UtcDateTime {
        year,
        month,
        day,
        hour: (day_seconds / 3600) as u32,
        minute: ((day_seconds % 3600) / 60) as u32,
        second: (day_seconds % 60) as u32,
    }
}

fn days_from_civil(year: i32, month: u32, day: u32) -> i64 {
    let year = i64::from(year) - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let yoe = year - era * 400;
    let month = i64::from(month);
    let doy = (153 * (month + if month > 2 { -3 } else { 9 }) + 2) / 5 + i64::from(day) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

fn civil_from_days(days: i64) -> (i32, u32, u32) {
    let days = days + 719_468;
    let era = days.div_euclid(146_097);
    let doe = days - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let year = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    let year = year + i64::from(month <= 2);
    (year as i32, month as u32, day as u32)
}

fn days_in_month(year: i32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap_year(year) => 29,
        2 => 28,
        _ => 0,
    }
}

fn is_leap_year(year: i32) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

#[cfg(test)]
mod tests {
    use super::{add_duration_hours, compare_iso_utc, parse_duration_hours, parse_iso_utc};

    #[test]
    fn parses_and_canonicalizes_utc_iso_timestamp() {
        assert_eq!(
            parse_iso_utc("2026-06-26T12:30:45Z").unwrap(),
            "2026-06-26T12:30:45Z"
        );
        assert!(parse_iso_utc("2026-02-29T00:00:00Z").is_err());
        assert!(parse_iso_utc("2026-06-26T12:30:45+06:00").is_err());
    }

    #[test]
    fn shifts_across_day_and_month_boundaries() {
        assert_eq!(
            add_duration_hours("2026-06-30T23:00:00Z", 2.0).unwrap(),
            "2026-07-01T01:00:00Z"
        );
    }

    #[test]
    fn parses_hour_duration_and_compares_timestamps() {
        assert_eq!(parse_duration_hours("1.5 h").unwrap(), 1.5);
        assert_eq!(
            compare_iso_utc("2026-06-26T12:00:00Z", "2026-06-26T13:00:00Z").unwrap(),
            std::cmp::Ordering::Less
        );
    }
}
