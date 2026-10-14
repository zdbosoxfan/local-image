//! One shortcut vocabulary for persisted profiles, Library, Editor and native menus.
//! The core has no UI dependency; the `egui` feature adapts the same chords to input events.

pub mod keymaps;
mod matching;
pub mod profiles;
pub use matching::{conflicts, global_conflict, global_shortcut_owner};
#[cfg(feature = "egui")]
pub mod input;
pub use profiles::{GLOBAL_SHORTCUTS, ShortcutProfile, ShortcutProfiles, ShortcutWorkspace};

/// Physical platform rules are explicit so both macOS and Ctrl-based platforms can be tested.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Platform {
    Mac,
    Ctrl,
}
impl Platform {
    pub const CURRENT: Self = if cfg!(target_os = "macos") { Self::Mac } else { Self::Ctrl };
}

/// A portable chord. Cmd is the platform command key; Ctrl stays physical Control on macOS.
/// Public fields preserve the native menu's existing API.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct Chord {
    pub cmd: bool,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
    pub key: String,
}
impl Chord {
    pub fn parse(s: &str) -> Option<Self> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let mut c = Self { cmd: false, ctrl: false, alt: false, shift: false, key: String::new() };
        // A trailing + key is written Cmd++; the normalized spelling keeps it.
        let plus = s == "+" || s.ends_with("++");
        let body = if plus { s.trim_end_matches('+') } else { s };
        for part in body.split('+').filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "cmd" | "command" | "⌘" | "meta" | "super" => c.cmd = true,
                "ctrl" | "control" | "⌃" => c.ctrl = true,
                "alt" | "option" | "opt" | "⌥" => c.alt = true,
                "shift" | "⇧" => c.shift = true,
                _ if c.key.is_empty() => c.key = shortcut_key(part)?,
                _ => return None,
            }
        }
        if plus {
            if !c.key.is_empty() {
                return None;
            }
            c.key = "+".into();
        }
        // Native accelerator consumers historically use the literal backquote spelling.
        if c.key == "Backtick" {
            c.key = "`".into();
        }
        (!c.key.is_empty()).then_some(c)
    }

    fn written(&self, native_order: bool) -> String {
        let mut out = String::new();
        let first = if native_order { [(self.ctrl, "Ctrl"), (self.cmd, "Cmd")] } else { [(self.cmd, "Cmd"), (self.ctrl, "Ctrl")] };
        for (on, name) in first.into_iter().chain([(self.alt, "Alt"), (self.shift, "Shift")]) {
            if on {
                out.push_str(name);
                out.push('+');
            }
        }
        out.push_str(if !native_order && self.key == "`" { "Backtick" } else { &self.key });
        out
    }
    /// Persisted notation retains the engine's Cmd+Ctrl+Alt+Shift order.
    pub fn normalized(&self) -> String {
        self.written(false)
    }
    /// Native menu notation retains its public Ctrl+Cmd+Alt+Shift order.
    pub fn portable(&self) -> String {
        self.written(true)
    }
    pub fn is_function_key(&self) -> bool {
        self.key.strip_prefix('F').and_then(|s| s.parse::<u8>().ok()).is_some_and(|n| (1..=24).contains(&n))
    }
    /// Bare and Alt/Shift-only tool keys must remain available to text fields.
    pub fn native_ok(&self) -> bool {
        self.cmd || self.ctrl || self.is_function_key()
    }
}

pub fn normalize_shortcut(s: &str) -> Option<String> {
    Some(Chord::parse(s)?.normalized())
}

/// Platform-independent key vocabulary. UI regression tests check every egui key against
/// this table, so persisted spellings stay executable without making the engine depend on egui.
fn shortcut_key(s: &str) -> Option<String> {
    let lower = s.to_ascii_lowercase();
    let canonical = match lower.as_str() {
        "arrowdown" | "down" | "⏷" => "Down",
        "arrowleft" | "left" | "⏴" => "Left",
        "arrowright" | "right" | "⏵" => "Right",
        "arrowup" | "up" | "⏶" => "Up",
        "esc" | "escape" => "Escape",
        "return" | "numpadenter" | "enter" => "Enter",
        "help" | "insert" => "Insert",
        "numpadcomma" | "comma" | "," => ",",
        "numpadsubtract" | "minus" | "−" | "-" => "-",
        "numpaddecimal" | "period" | "." => ".",
        "numpadadd" | "plus" | "+" => "+",
        "numpadequal" | "equal" | "equals" | "=" => "=",
        "colon" | ":" => ":",
        "semicolon" | ";" => ";",
        "backslash" | "\\" => "\\",
        "numpaddivide" | "slash" | "/" => "/",
        "pipe" | "|" => "|",
        "questionmark" | "?" => "?",
        "exclamationmark" | "!" => "!",
        "bracketleft" | "openbracket" | "[" => "[",
        "bracketright" | "closebracket" | "]" => "]",
        "opencurlybracket" | "{" => "OpenCurlyBracket",
        "closecurlybracket" | "}" => "CloseCurlyBracket",
        "backtick" | "backquote" | "grave" | "`" | "~" => "Backtick",
        "quote" | "'" => "'",
        "metaleft" | "osleft" => "SuperLeft",
        "metaright" | "osright" => "SuperRight",
        _ => {
            const NAMES: &[&str] = &[
                "Tab",
                "Backspace",
                "Delete",
                "Home",
                "End",
                "PageUp",
                "PageDown",
                "Copy",
                "Cut",
                "Paste",
                "Space",
                "BrowserBack",
                "ShiftLeft",
                "ShiftRight",
                "ControlLeft",
                "ControlRight",
                "AltLeft",
                "AltRight",
                "SuperLeft",
                "SuperRight",
                "IntlBackslash",
            ];
            if let Some(name) = NAMES.iter().find(|name| name.eq_ignore_ascii_case(s)) {
                return Some((*name).into());
            }
            let digit = lower.strip_prefix("digit").or_else(|| lower.strip_prefix("numpad")).unwrap_or(&lower);
            if digit.len() == 1 && digit.as_bytes()[0].is_ascii_digit() {
                return Some(digit.into());
            }
            let letter = lower.strip_prefix("key").unwrap_or(&lower);
            if letter.len() == 1 && letter.as_bytes()[0].is_ascii_alphabetic() {
                return Some(letter.to_ascii_uppercase());
            }
            if let Some(number) = lower.strip_prefix('f').and_then(|n| n.parse::<u8>().ok()).filter(|n| (1..=35).contains(n)) {
                return Some(format!("F{number}"));
            }
            return None;
        }
    };
    Some(canonical.into())
}

#[cfg(all(test, feature = "egui"))]
mod legacy_tests;
#[cfg(test)]
mod tests;
