//! Local offsets from the system TZif database (RFC 9636), without libc or a new dependency.
use std::path::Path;

pub(super) fn local_offset(seconds: i64) -> i32 {
    let zone = std::env::var("TZ").ok();
    if zone.as_deref() == Some("") {
        return 0;
    }
    let name = zone.as_deref().unwrap_or("/etc/localtime").trim_start_matches(':');
    let path = if Path::new(name).is_absolute() { name.into() } else { Path::new("/usr/share/zoneinfo").join(name) };
    std::fs::read(path).ok().and_then(|data| tzif_offset(&data, seconds)).or_else(|| posix_offset(name, seconds)).unwrap_or(0)
}

fn counts(header: &[u8]) -> Option<[usize; 6]> {
    if header.get(..4)? != b"TZif" {
        return None;
    }
    let mut result = [0; 6];
    for (i, n) in result.iter_mut().enumerate() {
        *n = u32::from_be_bytes(header.get(20 + 4 * i..24 + 4 * i)?.try_into().ok()?) as usize;
    }
    Some(result)
}
fn block_len(c: [usize; 6], width: usize) -> Option<usize> {
    c[3].checked_mul(width + 1)?
        .checked_add(c[4].checked_mul(6)?)?
        .checked_add(c[5])?
        .checked_add(c[2].checked_mul(width + 4)?)?
        .checked_add(c[0])?
        .checked_add(c[1])
}
fn timestamp(data: &[u8], width: usize, index: usize) -> Option<i64> {
    let value = data.get(index.checked_mul(width)?..index.checked_add(1)?.checked_mul(width)?)?;
    Some(if width == 8 { i64::from_be_bytes(value.try_into().ok()?) } else { i64::from(i32::from_be_bytes(value.try_into().ok()?)) })
}
fn tzif_offset(data: &[u8], seconds: i64) -> Option<i32> {
    let first = counts(data.get(..44)?)?;
    let (at, width) = if matches!(data[4], b'2' | b'3' | b'4') { (44usize.checked_add(block_len(first, 4)?)?, 8) } else { (0, 4) };
    let c = counts(data.get(at..at + 44)?)?;
    if c[4] == 0 || c[4] > 256 {
        return None;
    }
    let block = data.get(at + 44..at + 44 + block_len(c, width)?)?;
    if width == 8 && (c[3] == 0 || seconds > timestamp(block, width, c[3] - 1)?) {
        let tail = std::str::from_utf8(data.get(at + 44 + block.len()..)?).ok()?.trim_matches('\n');
        if let Some(offset) = posix_offset(tail, seconds) {
            return Some(offset);
        }
    }
    let (mut lo, mut hi) = (0, c[3]);
    while lo < hi {
        let mid = (lo + hi) / 2;
        if timestamp(block, width, mid)? <= seconds {
            lo = mid + 1;
        } else {
            hi = mid;
        }
    }
    let kind = if lo == 0 { 0 } else { usize::from(*block.get(c[3] * width + lo - 1)?) };
    if kind >= c[4] {
        return None;
    }
    let at = c[3] * (width + 1) + 6 * kind;
    Some(i32::from_be_bytes(block.get(at..at + 4)?.try_into().ok()?))
}

struct Parser<'a> {
    bytes: &'a [u8],
    at: usize,
}
impl Parser<'_> {
    fn peek(&self) -> Option<u8> {
        self.bytes.get(self.at).copied()
    }
    fn take(&mut self, b: u8) -> bool {
        if self.peek() == Some(b) {
            self.at += 1;
            true
        } else {
            false
        }
    }
    fn number(&mut self) -> Option<i64> {
        let start = self.at;
        let mut n = 0i64;
        while let Some(c @ b'0'..=b'9') = self.peek() {
            n = n.checked_mul(10)?.checked_add(i64::from(c - b'0'))?;
            self.at += 1;
        }
        (self.at > start).then_some(n)
    }
    fn name(&mut self) -> Option<()> {
        if self.take(b'<') {
            let start = self.at;
            while self.peek().is_some_and(|c| c != b'>') {
                self.at += 1;
            }
            if self.at > start && self.take(b'>') {
                return Some(());
            }
            return None;
        }
        let start = self.at;
        while self.peek().is_some_and(|c| c.is_ascii_alphabetic()) {
            self.at += 1;
        }
        (self.at - start >= 3).then_some(())
    }
    fn time(&mut self) -> Option<i64> {
        let sign = if self.take(b'-') {
            -1
        } else {
            self.take(b'+');
            1
        };
        let hours = self.number()?;
        let minutes = if self.take(b':') { self.number()? } else { 0 };
        let secs = if self.take(b':') { self.number()? } else { 0 };
        if minutes > 59 || secs > 59 || hours > 167 {
            return None;
        }
        Some(sign * (hours * 3600 + minutes * 60 + secs))
    }
    fn rule(&mut self) -> Option<Rule> {
        let day = if self.take(b'M') {
            let month = self.number()?;
            if !self.take(b'.') {
                return None;
            }
            let week = self.number()?;
            if !self.take(b'.') {
                return None;
            }
            let day = self.number()?;
            if !(1..=12).contains(&month) || !(1..=5).contains(&week) || !(0..=6).contains(&day) {
                return None;
            }
            Day::Month(month, week, day)
        } else if self.take(b'J') {
            let n = self.number()?;
            if !(1..=365).contains(&n) {
                return None;
            }
            Day::Julian(n)
        } else {
            let n = self.number()?;
            if !(0..=365).contains(&n) {
                return None;
            }
            Day::Ordinal(n)
        };
        let time = if self.take(b'/') { self.time()? } else { 7200 };
        let basis = if self.take(b's') {
            b's'
        } else if self.take(b'u') || self.take(b'g') || self.take(b'z') {
            b'u'
        } else {
            self.take(b'w');
            b'w'
        };
        Some(Rule { day, time, basis })
    }
}
enum Day {
    Month(i64, i64, i64),
    Julian(i64),
    Ordinal(i64),
}
struct Rule {
    day: Day,
    time: i64,
    basis: u8,
}
fn leap(year: i64) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}
fn days(year: i64, month: i64, day: i64) -> i64 {
    let year = year - i64::from(month <= 2);
    let era = year.div_euclid(400);
    let y = year - era * 400;
    let m = month + if month > 2 { -3 } else { 9 };
    let doy = (153 * m + 2) / 5 + day - 1;
    era * 146097 + y * 365 + y / 4 - y / 100 + doy - 719468
}
pub(super) fn civil(days: i64) -> (i64, i64, i64) {
    let z = days + 719468;
    let era = z.div_euclid(146097);
    let doe = z - era * 146097;
    let y = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let doy = doe - (365 * y + y / 4 - y / 100);
    let mp = (5 * doy + 2) / 153;
    let day = doy - (153 * mp + 2) / 5 + 1;
    let month = mp + if mp < 10 { 3 } else { -9 };
    (y + era * 400 + i64::from(month <= 2), month, day)
}
impl Rule {
    fn utc(&self, year: i64, standard: i64, before: i64) -> i64 {
        let day = match self.day {
            Day::Month(month, week, weekday) => {
                let first = days(year, month, 1);
                let mut d = 1 + (weekday - (first + 4).rem_euclid(7)).rem_euclid(7) + 7 * (week - 1);
                let next = if month == 12 { days(year + 1, 1, 1) } else { days(year, month + 1, 1) };
                if first + d > next {
                    d -= 7;
                }
                first + d - 1
            }
            Day::Julian(n) => days(year, 1, 1) + n - 1 + i64::from(leap(year) && n >= 60),
            Day::Ordinal(n) => days(year, 1, 1) + n,
        };
        day * 86400 + self.time
            - match self.basis {
                b'u' => 0,
                b's' => standard,
                _ => before,
            }
    }
}
fn posix_offset(text: &str, seconds: i64) -> Option<i32> {
    let mut p = Parser { bytes: text.as_bytes(), at: 0 };
    p.name()?;
    let standard = -p.time()?;
    if p.peek().is_none() {
        return i32::try_from(standard).ok();
    }
    p.name()?;
    let daylight = if p.peek() == Some(b',') { standard + 3600 } else { -p.time()? };
    if !p.take(b',') {
        return None;
    }
    let start = p.rule()?;
    if !p.take(b',') {
        return None;
    }
    let end = p.rule()?;
    if p.peek().is_some() {
        return None;
    }
    let year = civil(seconds.div_euclid(86400)).0;
    let start = start.utc(year, standard, standard);
    let end = end.utc(year, standard, daylight);
    let summer = if start < end { seconds >= start && seconds < end } else { seconds >= start || seconds < end };
    i32::try_from(if summer { daylight } else { standard }).ok()
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn ds_finding_local_offsets_cover_seasons_and_future_tzif_footer() {
        let eastern = "EST5EDT,M3.2.0,M11.1.0";
        assert_eq!(posix_offset(eastern, days(2026, 1, 1) * 86400), Some(-18000));
        assert_eq!(posix_offset(eastern, days(2026, 7, 1) * 86400), Some(-14400));
        let southern = "AEST-10AEDT,M10.1.0,M4.1.0/3";
        assert_eq!(posix_offset(southern, days(2026, 1, 1) * 86400), Some(39600));
        assert_eq!(posix_offset(southern, days(2026, 7, 1) * 86400), Some(36000));
        // A minimal v2 file with an expired transition and a current POSIX footer.
        let mut header = [0u8; 44];
        header[..5].copy_from_slice(b"TZif2");
        header[39] = 1;
        let mut data = header.to_vec();
        data.extend_from_slice(&[0; 6]);
        data.extend_from_slice(&header);
        data.extend_from_slice(&[0; 6]);
        data.extend_from_slice(format!("\n{eastern}\n").as_bytes());
        assert_eq!(tzif_offset(&data, days(2045, 7, 1) * 86400), Some(-14400));
        assert_eq!(tzif_offset(&data, days(2045, 1, 1) * 86400), Some(-18000));
    }
    #[test]
    fn ds_finding_tzif_truncation_and_invalid_rules_are_rejected() {
        for n in 0..44 {
            assert!(tzif_offset(&vec![0; n], 0).is_none());
        }
        assert!(posix_offset("EST5EDT,M13.1.0,M11.1.0", 0).is_none());
        assert_eq!(posix_offset("UTC0", 0), Some(0));
        assert_eq!(civil(days(2000, 2, 29)), (2000, 2, 29));
    }
}
