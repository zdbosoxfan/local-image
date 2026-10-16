use crate::{Chord, Platform};
use std::collections::BTreeMap;

// The same matcher serves persisted chords and egui logical key names without allocating.
fn token(key: &str) -> &str {
    match key {
        "Up" => "ArrowUp",
        "Down" => "ArrowDown",
        "Left" => "ArrowLeft",
        "Right" => "ArrowRight",
        "Delete" => "Backspace",
        "," => "Comma",
        "-" => "Minus",
        "." => "Period",
        "'" => "Quote",
        "`" => "Backtick",
        "+" => "Plus",
        "=" => "Equals",
        ";" => "Semicolon",
        ":" => "Colon",
        "[" => "OpenBracket",
        "]" => "CloseBracket",
        "/" => "Slash",
        "\\" => "Backslash",
        "?" => "Questionmark",
        "|" => "Pipe",
        "!" => "Exclamationmark",
        "1" => "Num1",
        _ => key,
    }
}
fn shifted(key: &str) -> Option<&'static str> {
    Some(match token(key) {
        "Equals" => "Plus",
        "Semicolon" => "Colon",
        "OpenBracket" => "OpenCurlyBracket",
        "CloseBracket" => "CloseCurlyBracket",
        "Slash" => "Questionmark",
        "Backslash" => "Pipe",
        "Num1" => "Exclamationmark",
        _ => return None,
    })
}
fn shifted_symbol(key: &str) -> bool {
    matches!(token(key), "Plus" | "Colon" | "OpenCurlyBracket" | "CloseCurlyBracket" | "Questionmark" | "Pipe" | "Exclamationmark")
}
// [logical command, physical control, alt, shift]
pub(crate) fn modifiers(mut mods: [bool; 4], platform: Platform) -> [bool; 4] {
    if platform == Platform::Ctrl {
        mods[0] |= mods[1];
        mods[1] = false;
    }
    mods
}
pub(crate) fn matches(pattern_key: &str, pattern: [bool; 4], actual_key: &str, actual: [bool; 4], platform: Platform) -> bool {
    let pattern = modifiers(pattern, platform);
    let actual = modifiers(actual, platform);
    let key = token(actual_key);
    let k = token(pattern_key);
    if !(key == k || (k == "Equals" && key == "Plus") || (pattern[3] && shifted(k) == Some(key))) {
        return false;
    }
    let shift_ok = actual[3] == pattern[3] || (shifted_symbol(key) && !pattern[3]);
    actual[..3] == pattern[..3] && shift_ok
}
pub(crate) fn overlaps(a_key: &str, a: [bool; 4], b_key: &str, b: [bool; 4], platform: Platform) -> bool {
    if modifiers(a, platform)[..3] != modifiers(b, platform)[..3] {
        return false;
    }
    let a_key = token(a_key);
    let b_key = token(b_key);
    let a_shifted = shifted(a_key);
    let b_shifted = shifted(b_key);
    if a_key != b_key && a_shifted != Some(b_key) && b_shifted != Some(a_key) && a_shifted.is_none_or(|key| Some(key) != b_shifted) {
        return false;
    }
    for key in [Some(a_key), Some(b_key), shifted(a_key), shifted(b_key)].into_iter().flatten() {
        for shift in [false, true] {
            let actual = [a[0], a[1], a[2], shift];
            if matches(a_key, a, key, actual, platform) && matches(b_key, b, key, actual, platform) {
                return true;
            }
        }
    }
    false
}
impl Chord {
    fn modifiers(&self) -> [bool; 4] {
        [self.cmd, self.ctrl, self.alt, self.shift]
    }
    pub fn overlaps_on(&self, other: &Self, platform: Platform) -> bool {
        overlaps(&self.key, self.modifiers(), &other.key, other.modifiers(), platform)
    }
    pub fn overlaps(&self, other: &Self) -> bool {
        self.overlaps_on(other, Platform::CURRENT)
    }
}

pub fn global_conflict(chord: &str) -> Option<(&'static str, &'static str)> {
    let chord = Chord::parse(chord)?;
    static GLOBAL: std::sync::LazyLock<Vec<Chord>> =
        std::sync::LazyLock::new(|| crate::GLOBAL_SHORTCUTS.iter().map(|(_, _, key)| Chord::parse(key).expect("fixed Global chord")).collect());
    crate::GLOBAL_SHORTCUTS.iter().zip(GLOBAL.iter()).find(|(_, global)| global.overlaps(&chord)).map(|((id, label, _), _)| (*id, *label))
}
pub fn global_shortcut_owner(chord: &str) -> Option<&'static str> {
    global_conflict(chord).map(|(id, _)| id)
}

/// Event-overlapping bindings grouped deterministically; repeated ids never conflict with themselves.
/// Connected groups also report a shifted-symbol binding that overlaps two other spellings.
pub fn conflicts<'a>(bindings: impl IntoIterator<Item = (&'a str, &'a str)>) -> Vec<(String, Vec<String>)> {
    let mut by = BTreeMap::<String, Vec<String>>::new();
    for (id, key) in bindings {
        if let Some(chord) = Chord::parse(key) {
            let ids = by.entry(chord.normalized()).or_default();
            if !ids.iter().any(|previous| previous == id) {
                ids.push(id.into());
            }
        }
    }
    let mut groups: Vec<(Chord, Vec<Chord>, Vec<String>)> = Vec::new();
    for (key, ids) in by {
        let chord = Chord::parse(&key).expect("already parsed");
        let mut members = vec![chord.clone()];
        let mut owners = ids;
        let mut first = chord;
        let mut n = 0;
        while n < groups.len() {
            if groups[n].1.iter().any(|a| members.iter().any(|b| a.overlaps(b))) {
                let (representative, old_members, old_owners) = groups.remove(n);
                if representative.normalized() < first.normalized() {
                    first = representative;
                }
                members.extend(old_members);
                for id in old_owners {
                    if !owners.contains(&id) {
                        owners.push(id);
                    }
                }
                // Newly joined members may connect an earlier group.
                n = 0;
            } else {
                n += 1;
            }
        }
        groups.push((first, members, owners));
    }
    let mut result: Vec<_> = groups.into_iter().filter(|(_, _, ids)| ids.len() > 1).map(|(chord, _, ids)| (chord.normalized(), ids)).collect();
    result.sort_by(|a, b| a.0.cmp(&b.0));
    result
}
