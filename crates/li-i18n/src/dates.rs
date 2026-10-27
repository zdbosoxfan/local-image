//! Localized date labels shared by the Library and editor.

use crate::{Lang, library};

/// A date group heading (`2026-09-20`, `2026-09`, `2026`) in the UI language: the grid's full
/// form, or the date sidebar's `short` one ("Sunday, 20" / "September" / "2026"). English keeps the
/// catalog's own wording; every other language formats it with its `*-formats.json` date
/// patterns and weekday names. The grouping keys themselves never change.
pub fn date_group_label(key: &str, short: bool) -> String {
    let label = crate::calendar::group_label(key);
    if crate::current() == Lang::En {
        return if short {
            match key.len() {
                // "September 2026" → "September"
                7 => label.split(' ').next().unwrap_or(&label).to_string(),
                // "Sunday, 20 September 2026" → "Sunday, 20"
                10 => label.rsplitn(3, ' ').nth(2).unwrap_or(&label).to_string(),
                _ => label,
            }
        } else {
            label
        };
    }
    let Some(year) = key.get(..4).and_then(|y| y.parse::<u32>().ok()) else { return library::t(&label).to_string() };
    if key.len() == 4 {
        return tr_format!("{year}", year = year);
    }
    let Some(month) = key.get(5..7).and_then(|m| m.parse::<u32>().ok()).filter(|m| (1..=12).contains(m)) else {
        return library::t(&label).to_string();
    };
    if key.len() == 7 {
        return if short { tr_format!("{month}", month = month) } else { tr_format!("{month} {year}", month = month, year = year) };
    }
    if key.len() == 10
        && let Some(weekday) = crate::calendar::weekday(key)
        && let Some(day) = key.get(8..10).and_then(|d| d.parse::<u32>().ok())
    {
        let weekday = library::t(weekday);
        return if short {
            tr_format!("{weekday}, {day}", weekday = weekday, day = day)
        } else {
            tr_format!("{weekday}, {day} {month} {year}", weekday = weekday, day = day, month = month, year = year)
        };
    }
    library::t(&label).to_string()
}

/// A capture time for display in the UI language (English: "March 30, 2022 at 10:11:11 PM");
/// the metadata itself is never rewritten, and a value that doesn't parse is shown as it is.
pub fn display_time(iso: &str) -> String {
    if crate::current() == Lang::En || crate::calendar::normalize_iso(iso).is_none() {
        return crate::calendar::display_time(iso);
    }
    let date = date_group_label(iso.get(..10).unwrap_or(iso), false);
    match iso.get(11..).filter(|time| !time.is_empty()) {
        Some(time) => format!("{date} {time}"),
        None => date,
    }
}
