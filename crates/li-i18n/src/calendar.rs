//! Calendar arithmetic and English display labels for ISO 8601 local times.

pub const WEEKDAYS: [&str; 7] = ["Monday", "Tuesday", "Wednesday", "Thursday", "Friday", "Saturday", "Sunday"];
pub const MONTHS: [&str; 12] = ["January", "February", "March", "April", "May", "June", "July", "August", "September", "October", "November", "December"];

/// Unix seconds → `YYYY-MM-DDTHH:MM:SS` (proleptic Gregorian; H. Hinnant's algorithm).
pub fn civil(secs: i64) -> String {
    let days = secs.div_euclid(86_400);
    let rem = secs.rem_euclid(86_400);
    let (y, m, d) = civil_from_days(days);
    format!("{y:04}-{m:02}-{d:02}T{:02}:{:02}:{:02}", rem / 3600, rem % 3600 / 60, rem % 60)
}

/// Days since 1970-01-01 → (year, month 1..=12, day 1..=31).
pub fn civil_from_days(days: i64) -> (i64, i64, i64) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    (yoe + era * 400 + i64::from(m <= 2), m, d)
}

/// Weekday name of an ISO date (`None` if it doesn't parse).
pub fn weekday(iso: &str) -> Option<&'static str> {
    let days = iso_seconds(iso.get(..10)?)?.div_euclid(86_400);
    // 1970-01-01 was a Thursday
    Some(WEEKDAYS[(days + 3).rem_euclid(7) as usize])
}

/// Shift an ISO time by `secs`, keeping anything after the seconds (fractions, zone) as it was.
/// A date without a time gets one. `None` if it doesn't parse, or if the result leaves years
/// 0000–9999 (an ISO date here has four year digits, so it couldn't be read back); `secs` may be
/// anything, it never overflows.
pub fn shift_iso(iso: &str, secs: i64) -> Option<String> {
    let t = iso_seconds(iso)?.checked_add(secs)?;
    let (first, last) = (iso_seconds("0000-01-01T00:00:00")?, iso_seconds("9999-12-31T23:59:59")?);
    if !(first..=last).contains(&t) {
        return None;
    }
    let tail = iso.trim().get(19..).unwrap_or("");
    Some(format!("{}{tail}", civil(t)))
}

/// Normalize user input to `YYYY-MM-DDTHH:MM:SS` (accepts a space instead of `T`, missing
/// seconds or time). `None` if it isn't a valid date.
pub fn normalize_iso(s: &str) -> Option<String> {
    let t = iso_seconds(s)?;
    let out = civil(t);
    // reject impossible dates that the arithmetic silently rolls over (2026-02-30)
    if out.get(..10) != s.trim().get(..10) {
        return None;
    }
    Some(out)
}

/// A capture time for display: `2022-03-30T22:11:11` → "March 30, 2022 at 10:11:11 PM" (the
/// date alone when there is no time; the input unchanged when it doesn't parse).
pub fn display_time(iso: &str) -> String {
    let num = |r: std::ops::Range<usize>| iso.get(r).and_then(|v| v.parse::<u32>().ok());
    let (Some(y), Some(mo), Some(d)) = (num(0..4), num(5..7), num(8..10)) else { return iso.to_string() };
    let Some(month) = MONTHS.get((mo as usize).wrapping_sub(1)) else { return iso.to_string() };
    let date = format!("{month} {d}, {y}");
    match (num(11..13), num(14..16), num(17..19)) {
        (Some(h), Some(mi), s) => {
            let (h12, ampm) = match h {
                0 => (12, "AM"),
                1..=11 => (h, "AM"),
                12 => (12, "PM"),
                _ => (h - 12, "PM"),
            };
            format!("{date} at {h12}:{mi:02}:{:02} {ampm}", s.unwrap_or(0))
        }
        _ => date,
    }
}

/// Header text for a group key.
pub fn group_label(key: &str) -> String {
    let month = |k: &str| k.get(5..7).and_then(|m| m.parse::<usize>().ok()).and_then(|m| MONTHS.get(m.wrapping_sub(1)));
    // `get` rather than `[..]`: a non-ASCII key must not split a character
    let year = key.get(..4).unwrap_or(key);
    match key.len() {
        10 => match (weekday(key), month(key), key.get(8..10).and_then(|d| d.parse::<u32>().ok())) {
            (Some(w), Some(m), Some(d)) => format!("{w}, {d} {m} {year}"),
            _ => key.to_string(),
        },
        7 => month(key).map(|m| format!("{m} {year}")).unwrap_or_else(|| key.to_string()),
        4 => key.to_string(),
        _ => "Unknown Date".into(),
    }
}

/// Seconds since 1970-01-01 for an ISO 8601 local time (`2026-04-01T10:00:00`, `2026-04-01 10:00`,
/// `2026-04-01`; fractions and zone suffixes are ignored). `None` if it doesn't parse.
pub fn iso_seconds(s: &str) -> Option<i64> {
    let s = s.trim();
    let num = |a: usize, b: usize| -> Option<i64> { s.get(a..b).filter(|x| x.bytes().all(|c| c.is_ascii_digit())).and_then(|x| x.parse().ok()) };
    let (y, m, d) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    if !(1..=12).contains(&m) || !(1..=31).contains(&d) {
        return None;
    }
    let (hh, mm, ss) = if s.len() >= 16 { (num(11, 13)?, num(14, 16)?, num(17, 19).unwrap_or(0)) } else { (0, 0, 0) };
    if hh >= 24 || mm >= 60 || ss >= 60 {
        return None;
    }
    // days from civil (proleptic Gregorian)
    let y2 = if m <= 2 { y - 1 } else { y };
    let era = y2.div_euclid(400);
    let yoe = y2 - era * 400;
    let mp = (m + 9) % 12;
    let doy = (153 * mp + 2) / 5 + d - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    let days = era * 146_097 + doe - 719_468;
    Some(days * 86_400 + hh * 3600 + mm * 60 + ss)
}
