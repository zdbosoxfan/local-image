//! Pure keyboard mapping and grid-selection movement for the Smart Sort review dialog.

use egui::{Key, Modifiers};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReviewAction {
    /// Move the focus by `dx` columns and `dy` rows. `extend` keeps the current anchor.
    Move(isize, isize, bool),
    /// Focus the first/last item.
    Home(bool),
    End(bool),
    SelectAll,
    /// Move the selection to the folder at this 0-based index.
    MoveTo(usize),
    /// Also add the selection to the folder at this 0-based index.
    AlsoAdd(usize),
    ToUnsorted,
    RemoveFromFolder,
    Undo,
    Redo,
    ClearSelection,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueAction {
    Yes,
    No,
    NotSure,
    Prev,
    Next,
    Undo,
}

fn no_mods(m: Modifiers) -> bool {
    m == Modifiers::NONE
}

fn shift_only(m: Modifiers) -> bool {
    m.shift && !m.alt && !m.command
}

fn command_only(m: Modifiers) -> bool {
    m.command && !m.alt && !m.shift
}

fn shift_command(m: Modifiers) -> bool {
    m.command && m.shift && !m.alt
}

fn alt_only(m: Modifiers) -> bool {
    m.alt && !m.shift && !m.command
}

fn number_index(key: Key) -> Option<usize> {
    match key {
        Key::Num1 => Some(0),
        Key::Num2 => Some(1),
        Key::Num3 => Some(2),
        Key::Num4 => Some(3),
        Key::Num5 => Some(4),
        Key::Num6 => Some(5),
        Key::Num7 => Some(6),
        Key::Num8 => Some(7),
        Key::Num9 => Some(8),
        _ => None,
    }
}

pub fn review_action(key: Key, m: Modifiers) -> Option<ReviewAction> {
    if let Some(i) = number_index(key) {
        if no_mods(m) {
            return Some(ReviewAction::MoveTo(i));
        }
        if alt_only(m) {
            return Some(ReviewAction::AlsoAdd(i));
        }
    }
    if key == Key::Num0 && no_mods(m) {
        return Some(ReviewAction::ToUnsorted);
    }

    match key {
        Key::ArrowLeft if no_mods(m) => Some(ReviewAction::Move(-1, 0, false)),
        Key::ArrowLeft if shift_only(m) => Some(ReviewAction::Move(-1, 0, true)),
        Key::ArrowRight if no_mods(m) => Some(ReviewAction::Move(1, 0, false)),
        Key::ArrowRight if shift_only(m) => Some(ReviewAction::Move(1, 0, true)),
        Key::ArrowUp if no_mods(m) => Some(ReviewAction::Move(0, -1, false)),
        Key::ArrowUp if shift_only(m) => Some(ReviewAction::Move(0, -1, true)),
        Key::ArrowDown if no_mods(m) => Some(ReviewAction::Move(0, 1, false)),
        Key::ArrowDown if shift_only(m) => Some(ReviewAction::Move(0, 1, true)),
        Key::Home if no_mods(m) => Some(ReviewAction::Home(false)),
        Key::Home if shift_only(m) => Some(ReviewAction::Home(true)),
        Key::End if no_mods(m) => Some(ReviewAction::End(false)),
        Key::End if shift_only(m) => Some(ReviewAction::End(true)),
        Key::A if command_only(m) => Some(ReviewAction::SelectAll),
        Key::Backspace | Key::Delete if no_mods(m) => Some(ReviewAction::RemoveFromFolder),
        Key::Z if no_mods(m) => Some(ReviewAction::Undo),
        Key::Z if command_only(m) => Some(ReviewAction::Undo),
        Key::Z if shift_command(m) => Some(ReviewAction::Redo),
        Key::Escape if no_mods(m) => Some(ReviewAction::ClearSelection),
        _ => None,
    }
}

pub fn queue_action(key: Key, m: Modifiers) -> Option<QueueAction> {
    match key {
        Key::Y if no_mods(m) => Some(QueueAction::Yes),
        Key::N if no_mods(m) => Some(QueueAction::No),
        Key::S if no_mods(m) => Some(QueueAction::NotSure),
        Key::ArrowLeft if no_mods(m) => Some(QueueAction::Prev),
        Key::ArrowRight if no_mods(m) => Some(QueueAction::Next),
        Key::Z if command_only(m) => Some(QueueAction::Undo),
        _ => None,
    }
}

/// The hint shown next to folder N in the folder list (`"1"`…`"9"`, `None` beyond 9).
pub fn folder_hint(index: usize) -> Option<&'static str> {
    const HINTS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
    HINTS.get(index).copied()
}

/// Apply a grid move: new focus index and selection given the current focus, anchor, selection,
/// item count and columns. Movement clamps at the ends and never wraps.
pub fn apply_move(
    focus: usize,
    anchor: usize,
    count: usize,
    columns: usize,
    dx: isize,
    dy: isize,
    extend: bool,
) -> (usize, usize, std::ops::RangeInclusive<usize>) {
    if count == 0 {
        return (0, 0, 0..=0);
    }

    let last = count - 1;
    let cols = if columns == 0 { 1 } else { columns } as isize;
    let moved = (focus as isize).saturating_add(dx).saturating_add(dy.saturating_mul(cols));
    let new_focus = moved.clamp(0, last as isize) as usize;
    let anchor = if extend { anchor.min(last) } else { new_focus };
    let range = if anchor <= new_focus { anchor..=new_focus } else { new_focus..=anchor };
    (new_focus, anchor, range)
}

#[cfg(test)]
mod tests {
    use super::*;
    use egui::{Key, Modifiers};

    fn mods(alt: bool, shift: bool, command: bool) -> Modifiers {
        Modifiers { alt, shift, command, ..Modifiers::NONE }
    }

    #[test]
    fn review_arrows_move_and_extend() {
        assert_eq!(review_action(Key::ArrowLeft, Modifiers::NONE), Some(ReviewAction::Move(-1, 0, false)));
        assert_eq!(review_action(Key::ArrowLeft, mods(false, true, false)), Some(ReviewAction::Move(-1, 0, true)));
        assert_eq!(review_action(Key::ArrowRight, Modifiers::NONE), Some(ReviewAction::Move(1, 0, false)));
        assert_eq!(review_action(Key::ArrowRight, mods(false, true, false)), Some(ReviewAction::Move(1, 0, true)));
        assert_eq!(review_action(Key::ArrowUp, Modifiers::NONE), Some(ReviewAction::Move(0, -1, false)));
        assert_eq!(review_action(Key::ArrowUp, mods(false, true, false)), Some(ReviewAction::Move(0, -1, true)));
        assert_eq!(review_action(Key::ArrowDown, Modifiers::NONE), Some(ReviewAction::Move(0, 1, false)));
        assert_eq!(review_action(Key::ArrowDown, mods(false, true, false)), Some(ReviewAction::Move(0, 1, true)));

        assert_eq!(review_action(Key::ArrowLeft, mods(true, false, false)), None);
        assert_eq!(review_action(Key::ArrowLeft, mods(false, false, true)), None);
    }

    #[test]
    fn review_home_end_and_select_all() {
        assert_eq!(review_action(Key::Home, Modifiers::NONE), Some(ReviewAction::Home(false)));
        assert_eq!(review_action(Key::Home, mods(false, true, false)), Some(ReviewAction::Home(true)));
        assert_eq!(review_action(Key::End, Modifiers::NONE), Some(ReviewAction::End(false)));
        assert_eq!(review_action(Key::End, mods(false, true, false)), Some(ReviewAction::End(true)));

        assert_eq!(review_action(Key::A, mods(false, false, true)), Some(ReviewAction::SelectAll));
        assert_eq!(review_action(Key::A, Modifiers::NONE), None);
        assert_eq!(review_action(Key::A, mods(false, true, true)), None);
    }

    #[test]
    fn review_digits_cover_move_also_add_and_unsorted() {
        for (key, index) in [
            (Key::Num1, 0),
            (Key::Num2, 1),
            (Key::Num3, 2),
            (Key::Num4, 3),
            (Key::Num5, 4),
            (Key::Num6, 5),
            (Key::Num7, 6),
            (Key::Num8, 7),
            (Key::Num9, 8),
        ] {
            assert_eq!(review_action(key, Modifiers::NONE), Some(ReviewAction::MoveTo(index)));
            assert_eq!(review_action(key, mods(true, false, false)), Some(ReviewAction::AlsoAdd(index)));
            assert_eq!(review_action(key, mods(false, true, false)), None, "Shift+digit must not move");
            assert_eq!(review_action(key, mods(false, false, true)), None, "Command+digit must not move");
        }

        assert_eq!(review_action(Key::Num0, Modifiers::NONE), Some(ReviewAction::ToUnsorted));
        assert_eq!(review_action(Key::Num0, mods(true, false, false)), None);
        assert_eq!(review_action(Key::Num0, mods(false, true, false)), None);
    }

    #[test]
    fn review_undo_redo_remove_and_clear() {
        assert_eq!(review_action(Key::Z, Modifiers::NONE), Some(ReviewAction::Undo));
        assert_eq!(review_action(Key::Z, mods(false, false, true)), Some(ReviewAction::Undo));
        assert_eq!(review_action(Key::Z, mods(false, true, true)), Some(ReviewAction::Redo));
        assert_eq!(review_action(Key::Z, mods(true, false, false)), None, "Alt+Z is not undo");
        assert_eq!(review_action(Key::Z, mods(false, true, false)), None);

        assert_eq!(review_action(Key::Escape, Modifiers::NONE), Some(ReviewAction::ClearSelection));
        assert_eq!(review_action(Key::Escape, mods(false, true, false)), None);

        assert_eq!(review_action(Key::Backspace, Modifiers::NONE), Some(ReviewAction::RemoveFromFolder));
        assert_eq!(review_action(Key::Delete, Modifiers::NONE), Some(ReviewAction::RemoveFromFolder));
        assert_eq!(review_action(Key::Backspace, mods(false, true, false)), None);
    }

    #[test]
    fn queue_mappings() {
        assert_eq!(queue_action(Key::Y, Modifiers::NONE), Some(QueueAction::Yes));
        assert_eq!(queue_action(Key::N, Modifiers::NONE), Some(QueueAction::No));
        assert_eq!(queue_action(Key::S, Modifiers::NONE), Some(QueueAction::NotSure));
        assert_eq!(queue_action(Key::ArrowLeft, Modifiers::NONE), Some(QueueAction::Prev));
        assert_eq!(queue_action(Key::ArrowRight, Modifiers::NONE), Some(QueueAction::Next));
        assert_eq!(queue_action(Key::Z, mods(false, false, true)), Some(QueueAction::Undo));

        assert_eq!(queue_action(Key::Y, mods(false, true, false)), None);
        assert_eq!(queue_action(Key::Z, Modifiers::NONE), None);
        assert_eq!(queue_action(Key::Z, mods(false, true, true)), None);
    }

    #[test]
    fn folder_hint_has_digits_through_nine() {
        const HINTS: [&str; 9] = ["1", "2", "3", "4", "5", "6", "7", "8", "9"];
        for (index, hint) in HINTS.iter().enumerate() {
            assert_eq!(folder_hint(index), Some(*hint));
        }
        assert_eq!(folder_hint(9), None);
        assert_eq!(folder_hint(100), None);
    }

    #[test]
    fn apply_move_clamps_and_selects() {
        assert_eq!(apply_move(3, 9, 0, 4, 1, 0, true), (0, 0, 0..=0));
        assert_eq!(apply_move(4, 9, 10, 5, 2, 0, false), (6, 6, 6..=6));
        assert_eq!(apply_move(4, 9, 10, 5, 1, 0, true), (5, 9, 5..=9));
        assert_eq!(apply_move(7, 2, 10, 5, -1, 0, true), (6, 2, 2..=6));
        assert_eq!(apply_move(0, 0, 5, 3, -1, 0, true), (0, 0, 0..=0));
        assert_eq!(apply_move(4, 0, 5, 3, 1, 0, true), (4, 0, 0..=4));
    }

    #[test]
    fn apply_move_rows_use_columns() {
        assert_eq!(apply_move(4, 4, 6, 3, 0, -1, false), (1, 1, 1..=1));
        assert_eq!(apply_move(1, 1, 6, 3, 0, 1, false), (4, 4, 4..=4));
        assert_eq!(apply_move(2, 2, 6, 3, -1, 0, false), (1, 1, 1..=1));
    }

    #[test]
    fn apply_move_zero_columns_falls_back_to_one() {
        assert_eq!(apply_move(2, 2, 5, 0, 0, 1, false), (3, 3, 3..=3));
    }
}
