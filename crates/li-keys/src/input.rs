//! egui adapters use logical keys (keyboard layout aware), never physical scan codes.
use crate::{Chord, Platform};
use egui::{Key, KeyboardShortcut, Modifiers};

impl Chord {
    pub fn shortcut(&self) -> Option<KeyboardShortcut> {
        Some(KeyboardShortcut::new(
            Modifiers { command: self.cmd, ctrl: self.ctrl, alt: self.alt, shift: self.shift, mac_cmd: false },
            Key::from_name(&self.key)?,
        ))
    }
    /// Native menu events represent the platform's physical command key as egui does.
    pub fn key_event_on(&self, pressed: bool, platform: Platform) -> Option<egui::Event> {
        let sc = self.shortcut()?;
        let modifiers = match platform {
            Platform::Mac => Modifiers { mac_cmd: self.cmd, ..sc.modifiers },
            Platform::Ctrl => Modifiers { ctrl: self.cmd || self.ctrl, command: self.cmd || self.ctrl, ..sc.modifiers },
        };
        Some(egui::Event::Key { key: sc.logical_key, physical_key: Some(sc.logical_key), pressed, repeat: false, modifiers })
    }
    pub fn key_event(&self, pressed: bool) -> Option<egui::Event> {
        self.key_event_on(pressed, Platform::CURRENT)
    }
}

pub fn parse(s: &str) -> Option<KeyboardShortcut> {
    Chord::parse(s)?.shortcut()
}

pub fn identity_on(mut modifiers: Modifiers, mut key: Key, platform: Platform) -> (Modifiers, Key) {
    match platform {
        Platform::Mac => modifiers.command |= modifiers.mac_cmd,
        Platform::Ctrl => {
            modifiers.command |= modifiers.ctrl;
            modifiers.ctrl = false;
        }
    }
    modifiers.mac_cmd = false;
    if key == Key::Delete {
        key = Key::Backspace;
    }
    (modifiers, key)
}
pub fn identity(modifiers: Modifiers, key: Key) -> (Modifiers, Key) {
    identity_on(modifiers, key, Platform::CURRENT)
}

fn parts(mods: Modifiers) -> [bool; 4] {
    [mods.command || mods.mac_cmd, mods.ctrl, mods.alt, mods.shift]
}

/// Exact physical Control on Mac, with layout-aware punctuation and Delete aliases.
pub fn key_matches_on(sc: &KeyboardShortcut, key: Key, mods: Modifiers, platform: Platform) -> bool {
    crate::matching::matches(sc.logical_key.name(), parts(sc.modifiers), key.name(), parts(mods), platform)
}
pub fn key_matches(sc: &KeyboardShortcut, key: Key, mods: Modifiers) -> bool {
    key_matches_on(sc, key, mods, Platform::CURRENT)
}

/// Two bindings conflict when at least one logical key event would invoke both.
pub fn overlaps_on(a: &KeyboardShortcut, b: &KeyboardShortcut, platform: Platform) -> bool {
    crate::matching::overlaps(a.logical_key.name(), parts(a.modifiers), b.logical_key.name(), parts(b.modifiers), platform)
}

pub fn overlaps(a: &KeyboardShortcut, b: &KeyboardShortcut) -> bool {
    overlaps_on(a, b, Platform::CURRENT)
}

/// First binding wins, after callers have placed fixed Global and custom bindings first.
pub fn retain_first<T>(bindings: &mut Vec<T>, chord: impl Fn(&T) -> &str) {
    let mut seen = Vec::new();
    bindings.retain(|binding| {
        let Some(sc) = parse(chord(binding)) else {
            return false;
        };
        if seen.iter().any(|previous| overlaps(&sc, previous)) {
            return false;
        }
        seen.push(sc);
        true
    });
}

/// Consume presses only, leaving releases available to temporary-tool lifecycle handling.
pub fn consume(ctx: &egui::Context, sc: &KeyboardShortcut) -> bool {
    ctx.input_mut(|i| {
        let before = i.events.len();
        i.events.retain(|e| !matches!(e, egui::Event::Key { key, pressed: true, modifiers, .. } if key_matches(sc, *key, *modifiers)));
        before != i.events.len()
    })
}

/// Remove the next matching press in event order. Callers stop polling when a command
/// transfers focus to a dialog/tool; unmatched events and all releases remain available.
pub fn take_pressed<T: Clone>(ctx: &egui::Context, bindings: &[(T, KeyboardShortcut)]) -> Option<(T, Key)> {
    ctx.input_mut(|i| {
        let (at, binding, key) = i.events.iter().enumerate().find_map(|(at, event)| {
            let egui::Event::Key { key, pressed: true, modifiers, .. } = event else {
                return None;
            };
            let (binding, _) = bindings.iter().find(|(_, sc)| key_matches(sc, *key, *modifiers))?;
            Some((at, binding.clone(), *key))
        })?;
        i.events.remove(at);
        Some((binding, key))
    })
}

pub fn is_modifier_key(key: Key) -> bool {
    matches!(key, Key::ShiftLeft | Key::ShiftRight | Key::ControlLeft | Key::ControlRight | Key::AltLeft | Key::AltRight | Key::SuperLeft | Key::SuperRight)
}
/// egui's full logical function-key vocabulary, including user-assigned F16–F35.
pub fn is_function_key(key: Key) -> bool {
    key.name().strip_prefix('F').and_then(|n| n.parse::<u8>().ok()).is_some_and(|n| (1..=35).contains(&n))
}
/// Capture uses the same portable platform mapping as parsing and dispatch.
pub fn shortcut_text_on(key: Key, mods: Modifiers, platform: Platform) -> Option<String> {
    if is_modifier_key(key) {
        return None;
    }
    let (mods, _) = identity_on(mods, key, platform);
    let mut chord = Chord::parse(key.name())?;
    chord.cmd = mods.command;
    chord.ctrl = mods.ctrl;
    chord.alt = mods.alt;
    chord.shift = mods.shift;
    Some(chord.normalized())
}
pub fn shortcut_text(key: Key, mods: Modifiers) -> Option<String> {
    shortcut_text_on(key, mods, Platform::CURRENT)
}
