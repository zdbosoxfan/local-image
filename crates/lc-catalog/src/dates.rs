//! Calendar helpers for ISO 8601 local times (`2026-09-30T14:05:00`): seconds ↔ civil date,
//! weekday and month names, shifting a time by an offset, and grouping a photo list into runs of
//! the same day / month / year (the date headers of the photo grid).

use serde::{Deserialize, Serialize};

pub use crate::stacks::iso_seconds;
use crate::{Catalog, PhotoId, SortKey};

pub use li_i18n::calendar::{MONTHS, WEEKDAYS, civil, civil_from_days, display_time, group_label, normalize_iso, shift_iso, weekday};

/// Granularity of the grid's date headers.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum GroupBy {
    /// Day headers, or months/years when the thumbnails are small (decided by the UI).
    #[default]
    Auto,
    None,
    Day,
    Month,
    Year,
}

impl GroupBy {
    pub fn parse(s: &str) -> Option<GroupBy> {
        Some(match s.to_ascii_lowercase().as_str() {
            "auto" => GroupBy::Auto,
            "none" | "off" => GroupBy::None,
            "day" => GroupBy::Day,
            "month" => GroupBy::Month,
            "year" => GroupBy::Year,
            _ => return None,
        })
    }
    /// Length of the ISO prefix that identifies a group.
    fn prefix(self) -> usize {
        match self {
            GroupBy::Year => 4,
            GroupBy::Month => 7,
            _ => 10,
        }
    }
}

/// A run of consecutive photos with the same date (one header in the grid).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct DateRun {
    /// `2026-09-30` / `2026-09` / `2026`, or empty for photos without a usable date.
    pub key: String,
    /// "Wednesday, 30 September 2026" / "September 2026" / "2026" / "Unknown Date".
    pub label: String,
    /// Index of the first photo in the list.
    pub start: usize,
    pub count: usize,
}

impl Catalog {
    /// Split `ids` (the grid order) into runs of the same day / month / year for `key`'s date.
    /// Members of a stack share the group of the stack's first photo in the list, so a stack is
    /// never split by a header. Empty for sort keys without a date and for [`GroupBy::None`]
    /// ([`GroupBy::Auto`] means days here).
    pub fn date_runs(&self, ids: &[PhotoId], key: SortKey, by: GroupBy) -> Vec<DateRun> {
        if by == GroupBy::None || ids.is_empty() {
            return Vec::new();
        }
        let n = by.prefix();
        let date_of = |id: &PhotoId| -> String {
            let Some(p) = self.photo(*id) else { return String::new() };
            let d = match key {
                SortKey::CaptureDate => Some(p.date()),
                SortKey::ImportDate => Some(p.imported.as_str()),
                SortKey::EditDate => p.edited.as_deref(),
                _ => None,
            };
            d.filter(|d| iso_seconds(d.get(..10).unwrap_or("")).is_some()).and_then(|d| d.get(..n)).unwrap_or("").to_string()
        };
        if !matches!(key, SortKey::CaptureDate | SortKey::ImportDate | SortKey::EditDate) {
            return Vec::new();
        }
        let stacks = self.stack_index();
        let mut runs: Vec<DateRun> = Vec::new();
        let mut stack_key: std::collections::HashMap<crate::StackId, String> = Default::default();
        for (i, id) in ids.iter().enumerate() {
            let k = match stacks.get(id) {
                Some((sid, _)) => stack_key.entry(*sid).or_insert_with(|| date_of(id)).clone(),
                None => date_of(id),
            };
            match runs.last_mut() {
                Some(r) if r.key == k => r.count += 1,
                _ => runs.push(DateRun { label: group_label(&k), key: k, start: i, count: 1 }),
            }
        }
        runs
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{Op, Photo, Source};

    #[test]
    fn impossible_times_are_rejected_not_rolled_over() {
        for bad in ["2026-04-01T10:75:00", "2026-04-01 10:00:75", "2026-04-01T10:60", "2026-04-01T24:00:00", "2026-04-01T10:59:60"] {
            assert_eq!(normalize_iso(bad), None, "{bad}");
            assert_eq!(shift_iso(bad, 5), None, "{bad}");
        }
        assert_eq!(normalize_iso("2026-04-01T23:59:59").as_deref(), Some("2026-04-01T23:59:59"));
    }

    #[test]
    fn calendar() {
        assert_eq!(civil(0), "1970-01-01T00:00:00");
        assert_eq!(civil(951_782_400), "2000-02-29T00:00:00");
        assert_eq!(weekday("2026-09-30"), Some("Wednesday"));
        assert_eq!(weekday("2026-10-01T09:00:00"), Some("Thursday"));
        assert_eq!(weekday("2000-02-29"), Some("Tuesday"));
        assert_eq!(group_label("2026-09-29"), "Tuesday, 29 September 2026");
        assert_eq!(group_label("2026-02"), "February 2026");
        assert_eq!(group_label("2026"), "2026");
        assert_eq!(group_label(""), "Unknown Date");
        // non-ASCII keys of a date's length used to be sliced mid-character
        assert_eq!(group_label("2024-01é0"), "2024-01é0");
        assert_eq!(group_label("202é01"), "January 202é01");
        assert_eq!(shift_iso("2026-12-31T23:30:00", 3600).as_deref(), Some("2027-01-01T00:30:00"));
        assert_eq!(shift_iso("2026-03-01T00:00:00.25+02:00", -1).as_deref(), Some("2026-02-28T23:59:59.25+02:00"));
        assert_eq!(shift_iso("nope", 5), None);
        // upstream a08d0f6: hostile shifts never overflow, and a result outside years 0000–9999
        // (which no ISO date here can read back) is refused rather than stored
        for secs in [i64::MAX, i64::MIN, i64::MAX / 2, -(i64::MAX / 2)] {
            assert_eq!(shift_iso("2026-01-01T00:00:00", secs), None, "{secs}");
        }
        assert_eq!(shift_iso("9999-12-31T23:59:59", 1), None);
        assert_eq!(shift_iso("0000-01-01T00:00:00", -1), None);
        assert_eq!(shift_iso("9999-12-31T23:59:58", 1).as_deref(), Some("9999-12-31T23:59:59"));
        assert_eq!(normalize_iso("2026-04-01 10:05").as_deref(), Some("2026-04-01T10:05:00"));
        assert_eq!(normalize_iso("2026-02-30"), None);
    }

    #[test]
    fn runs_by_day_month_year_keep_stacks_together() {
        let mut c = Catalog::new();
        let times = ["2026-09-30T18:00:00", "2026-09-30T09:00:00", "2026-09-29T23:59:00", "2026-08-01T10:00:00", "2025-12-31T10:00:00", ""];
        let mut ids = Vec::new();
        for t in times {
            let id = c.alloc_photo_id();
            let mut p = Photo::new(id, Source::Demo { scene: 1 }, "a.jpg", "JPEG", 3, 2, "2026-10-01T00:00:00");
            p.captured = (!t.is_empty()).then(|| t.to_string());
            c.apply(Op::AddPhoto { photo: Box::new(p) }).unwrap();
            ids.push(id);
        }
        let shape = |r: &[DateRun]| r.iter().map(|r| (r.key.clone(), r.start, r.count)).collect::<Vec<_>>();
        let day = c.date_runs(&ids, SortKey::CaptureDate, GroupBy::Day);
        // the undated photo falls back to its import date
        assert_eq!(
            shape(&day),
            vec![
                ("2026-09-30".into(), 0, 2),
                ("2026-09-29".into(), 2, 1),
                ("2026-08-01".into(), 3, 1),
                ("2025-12-31".into(), 4, 1),
                ("2026-10-01".into(), 5, 1)
            ]
        );
        assert_eq!(day[0].label, "Wednesday, 30 September 2026");
        assert_eq!(c.date_runs(&ids, SortKey::CaptureDate, GroupBy::Auto), day);
        assert_eq!(
            shape(&c.date_runs(&ids[..5], SortKey::CaptureDate, GroupBy::Month)),
            vec![("2026-09".into(), 0, 3), ("2026-08".into(), 3, 1), ("2025-12".into(), 4, 1)]
        );
        assert_eq!(shape(&c.date_runs(&ids[..5], SortKey::CaptureDate, GroupBy::Year)), vec![("2026".into(), 0, 4), ("2025".into(), 4, 1)]);
        assert!(c.date_runs(&ids, SortKey::FileName, GroupBy::Day).is_empty());
        assert!(c.date_runs(&ids, SortKey::CaptureDate, GroupBy::None).is_empty());
        // edit date: unedited photos have no date
        let r = c.date_runs(&ids, SortKey::EditDate, GroupBy::Day);
        assert_eq!(shape(&r), vec![(String::new(), 0, 6)]);
        assert_eq!(r[0].label, "Unknown Date");
        // a stack spanning midnight stays in its first member's group
        let op = c.group_ops(ids[1], &[ids[1], ids[2]], false).unwrap();
        c.apply(op).unwrap();
        let arranged = c.arrange_stacks(&ids[..5]);
        assert_eq!(
            shape(&c.date_runs(&arranged, SortKey::CaptureDate, GroupBy::Day))[..2],
            [("2026-09-30".into(), 0, 3), ("2026-08-01".into(), 3, 1)]
        );
    }
}
