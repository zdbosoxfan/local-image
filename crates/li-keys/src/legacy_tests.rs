// Frozen J20 pre-merge references, retained ONLY for golden/parity tests.
#![allow(dead_code)]
pub mod engine {
    #[allow(unused_imports)]
    use egui::{Key, KeyboardShortcut, Modifiers};
    pub fn normalize(s: &str) -> Option<String> {
        let s = s.trim();
        if s.is_empty() {
            return None;
        }
        let (mut cmd, mut ctrl, mut alt, mut shift) = (false, false, false, false);
        let mut key: Option<String> = None;
        // A trailing "+" key is spelled "Cmd++".
        let (body, plus_key) = if s.ends_with("++") || s == "+" { (s.trim_end_matches('+'), true) } else { (s, false) };
        for part in body.split('+').filter(|p| !p.is_empty()) {
            match part.to_ascii_lowercase().as_str() {
                "cmd" | "command" | "⌘" | "meta" | "super" => cmd = true,
                "ctrl" | "control" | "⌃" => ctrl = true,
                "alt" | "option" | "opt" | "⌥" => alt = true,
                "shift" | "⇧" => shift = true,
                _ => {
                    if key.is_some() {
                        return None;
                    }
                    key = Some(shortcut_key(part)?);
                }
            }
        }
        if plus_key {
            if key.is_some() {
                return None;
            }
            key = Some("+".into());
        }
        let key = key?;
        let mut out = Vec::new();
        if cmd {
            out.push("Cmd");
        }
        if ctrl {
            out.push("Ctrl");
        }
        if alt {
            out.push("Alt");
        }
        if shift {
            out.push("Shift");
        }
        let mut s = out.join("+");
        if !s.is_empty() {
            s.push('+');
        }
        s.push_str(&key);
        Some(s)
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
            "backtick" | "backquote" | "grave" | "`" => "Backtick",
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
}
pub mod library {
    #[allow(unused_imports)]
    use egui::{Key, KeyboardShortcut, Modifiers};
    pub fn parse(s: &str) -> Option<(Modifiers, Key)> {
        let mut m = Modifiers::NONE;
        let mut key = None;
        let plus = s == "+" || s.ends_with("++");
        let body = if plus { s.trim_end_matches('+') } else { s };
        for part in body.split('+').filter(|part| !part.is_empty()) {
            match part {
                "Cmd" => m.command = true,
                "Shift" => m.shift = true,
                "Alt" => m.alt = true,
                "Ctrl" => m.ctrl = true,
                // "Delete" means the key labelled ⌫ (egui's Backspace); forward-delete also matches, see `matches`.
                "Delete" => {
                    if key.is_some() {
                        return None;
                    }
                    key = Some(Key::Backspace);
                }
                k => {
                    if key.is_some() {
                        return None;
                    }
                    key = Some(Key::from_name(k).or(match k {
                        "Right" => Some(Key::ArrowRight),
                        "Left" => Some(Key::ArrowLeft),
                        "Up" => Some(Key::ArrowUp),
                        "Down" => Some(Key::ArrowDown),
                        "\\" => Some(Key::Backslash),
                        "/" => Some(Key::Slash),
                        "=" => Some(Key::Equals),
                        "-" => Some(Key::Minus),
                        "[" => Some(Key::OpenBracket),
                        "]" => Some(Key::CloseBracket),
                        "'" => Some(Key::Quote),
                        "," => Some(Key::Comma),
                        _ => None,
                    })?)
                }
            }
        }
        if plus {
            if key.is_some() {
                return None;
            }
            key = Some(Key::Plus);
        }
        key.map(|k| (m, k))
    }

    /// One exact identity for dispatch, row navigation, held actions and collision precedence.
    pub fn chord_identity(mut modifiers: Modifiers, mut key: Key) -> (Modifiers, Key) {
        if !cfg!(target_os = "macos") {
            modifiers.command |= modifiers.ctrl;
            modifiers.ctrl = false;
        } else {
            modifiers.command |= modifiers.mac_cmd;
        }
        modifiers.mac_cmd = false;
        if key == Key::Delete {
            key = Key::Backspace;
        }
        (modifiers, key)
    }
    pub fn key_matches(pattern: Modifiers, key: Key, actual: Modifiers, actual_key: Key) -> bool {
        chord_identity(pattern, key) == chord_identity(actual, actual_key)
    }
}
pub mod editor {
    #[allow(unused_imports)]
    use egui::{Key, KeyboardShortcut, Modifiers};
    pub fn parse(s: &str) -> Option<KeyboardShortcut> {
        let normalized = super::engine::normalize(s)?;
        let s = normalized.as_str();
        let mut mods = Modifiers::NONE;
        let mut key = None;
        for part in s.split('+') {
            match part {
                "Cmd" => mods |= Modifiers::COMMAND,
                "Shift" => mods |= Modifiers::SHIFT,
                "Alt" => mods |= Modifiers::ALT,
                "Ctrl" => mods |= Modifiers::CTRL,
                "" => key = Some(Key::Plus),
                k => {
                    key = Key::from_name(k).or(match k {
                        "=" => Some(Key::Equals),
                        "-" => Some(Key::Minus),
                        "[" => Some(Key::OpenBracket),
                        ";" => Some(Key::Semicolon),
                        "'" => Some(Key::Quote),
                        "]" => Some(Key::CloseBracket),
                        _ => None,
                    })
                }
            }
        }
        Some(KeyboardShortcut::new(mods, key?))
    }

    fn shifted(k: Key) -> Option<Key> {
        Some(match k {
            Key::Equals => Key::Plus,
            Key::Semicolon => Key::Colon,
            Key::OpenBracket => Key::OpenCurlyBracket,
            Key::CloseBracket => Key::CloseCurlyBracket,
            Key::Slash => Key::Questionmark,
            Key::Backslash => Key::Pipe,
            Key::Num1 => Key::Exclamationmark,
            _ => return None,
        })
    }

    /// Does a key press match a shortcut, the way Photoshop matches them?
    ///
    /// - Modifiers match exactly (⌘⌥I is not ⌘I), with ⌘ = Ctrl off the Mac.
    /// - `=` also matches `+` (⌘+ with ⇧ on a US layout, the numpad `+`, the `+` key of Nordic and
    ///   German layouts): Photoshop zooms in on both.
    /// - Delete also matches Backspace (the Mac's "delete" key), and Backspace matches Delete (⌥⌫
    ///   and ⌥Delete both fill on Windows).
    /// - A ⇧ shortcut on a punctuation key matches the shifted character (⌘⇧; arrives as ⌘`:`).
    pub fn key_matches(sc: &KeyboardShortcut, key: Key, mods: Modifiers) -> bool {
        let k = sc.logical_key;
        let direct = key == k || (k == Key::Delete && key == Key::Backspace) || (k == Key::Backspace && key == Key::Delete);
        let plus = k == Key::Equals && key == Key::Plus;
        let via_shift = sc.modifiers.shift && shifted(k) == Some(key);
        if !(direct || plus || via_shift) {
            return false;
        }
        // Typing `+` (or another shifted symbol) may need ⇧ on this layout: don't require it absent.
        let shifted_symbol =
            matches!(key, Key::Plus | Key::Colon | Key::OpenCurlyBracket | Key::CloseCurlyBracket | Key::Questionmark | Key::Pipe | Key::Exclamationmark);
        let shift_ok = mods.shift == sc.modifiers.shift || (shifted_symbol && !sc.modifiers.shift);
        mods.alt == sc.modifiers.alt && shift_ok && mods.cmd_ctrl_matches(sc.modifiers)
    }
}
pub mod native {
    #[allow(unused_imports)]
    use egui::{Key, KeyboardShortcut, Modifiers};
    #[derive(Clone, Debug, PartialEq, Eq, Hash)]
    pub struct Chord {
        pub cmd: bool,
        pub ctrl: bool,
        pub alt: bool,
        pub shift: bool,
        /// The key as written: `Z`, `1`, `'`, `F5`, `Delete`, `Enter`.
        pub key: String,
    }

    impl Chord {
        pub fn parse(s: &str) -> Option<Chord> {
            let mut c = Chord { cmd: false, ctrl: false, alt: false, shift: false, key: String::new() };
            for part in s.split('+') {
                match part {
                    "Cmd" => c.cmd = true,
                    "Ctrl" => c.ctrl = true,
                    "Alt" => c.alt = true,
                    "Shift" => c.shift = true,
                    // `Cmd++` splits into an empty last part.
                    "" => c.key = "+".into(),
                    k if c.key.is_empty() => c.key = k.into(),
                    _ => return None,
                }
            }
            (!c.key.is_empty()).then_some(c)
        }

        /// The written form, modifiers in a fixed order.
        pub fn portable(&self) -> String {
            let mut parts = Vec::new();
            for (on, name) in [(self.ctrl, "Ctrl"), (self.cmd, "Cmd"), (self.alt, "Alt"), (self.shift, "Shift")] {
                if on {
                    parts.push(name);
                }
            }
            parts.push(&self.key);
            parts.join("+")
        }

        pub fn is_function_key(&self) -> bool {
            self.key.strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=24).contains(&n))
        }

        /// May the native menu show this as a key equivalent? Only with ⌘ or ⌃, or a function key.
        /// Bare letters and ⌥/⇧-only keys are tool keys: as key equivalents AppKit would take a `b`
        /// typed into a text field.
        pub fn native_ok(&self) -> bool {
            self.cmd || self.ctrl || self.is_function_key()
        }

        /// The key press egui would have received for this chord.
        pub fn key_event(&self, pressed: bool) -> Option<egui::Event> {
            let key = match self.key.as_str() {
                "+" => Key::Plus,
                "'" => Key::Quote,
                ";" => Key::Semicolon,
                "=" => Key::Equals,
                "-" => Key::Minus,
                "[" => Key::OpenBracket,
                "]" => Key::CloseBracket,
                "\\" => Key::Backslash,
                "/" => Key::Slash,
                "," => Key::Comma,
                "." => Key::Period,
                k => Key::from_name(k)?,
            };
            let modifiers = Modifiers { alt: self.alt, ctrl: self.ctrl, shift: self.shift, mac_cmd: self.cmd, command: self.cmd };
            Some(egui::Event::Key { key, physical_key: Some(key), pressed, repeat: false, modifiers })
        }
    }
}
