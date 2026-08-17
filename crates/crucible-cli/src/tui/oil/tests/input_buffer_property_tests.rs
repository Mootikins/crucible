//! Property tests for [`InputBuffer`] under arbitrary Unicode input.
//!
//! The example-based tests in `event_tests.rs` are all ASCII, which is why a
//! char-boundary panic in the word motions shipped: every offset in an ASCII
//! string is a boundary, so the arithmetic looked right. These tests drive the
//! whole [`InputAction`] set over strings that deliberately contain multi-byte
//! whitespace (U+00A0, U+2009, U+3000), CJK, emoji and combining marks, and
//! assert the buffer's structural invariant after *every* action.

use crate::tui::oil::{InputAction, InputBuffer};
use proptest::prelude::*;

/// Resolve the per-property case budget from the environment, matching the
/// `CRUCIBLE_PROPTEST_CASES` convention documented in
/// `crates/crucible-oil/tests/common/mod.rs`.
fn default_cases() -> u32 {
    std::env::var("CRUCIBLE_PROPTEST_CASES")
        .ok()
        .and_then(|s| s.parse().ok())
        .filter(|&n: &u32| n > 0)
        .unwrap_or(64)
}

/// Whitespace that is more than one byte wide. `char::is_whitespace` matches
/// all of these, so all of them can appear as a word separator.
const MULTIBYTE_WHITESPACE: &[char] = &[
    '\u{0085}', // NEL, 2 bytes
    '\u{00a0}', // NO-BREAK SPACE, 2 bytes
    '\u{2009}', // THIN SPACE, 3 bytes
    '\u{2028}', // LINE SEPARATOR, 3 bytes
    '\u{3000}', // IDEOGRAPHIC SPACE, 3 bytes
];

/// Non-ASCII characters that are *not* whitespace, so words themselves are
/// multi-byte and slicing at a word start is exercised too.
const WIDE_CHARS: &[char] = &[
    '日', '本', '한', 'é', 'ß', 'Ω', '😀', '👍', '\u{0301}', // combining acute
    '\u{0308}', // combining diaeresis
    '\u{200d}', // zero-width joiner
    '\u{fe0f}', // variation selector-16
];

fn interesting_char() -> impl Strategy<Value = char> {
    prop_oneof![
        4 => prop::char::range('a', 'z'),
        3 => prop::sample::select(MULTIBYTE_WHITESPACE),
        3 => prop::sample::select(WIDE_CHARS),
        2 => prop::sample::select(vec![' ', '\t', '\n']),
        1 => any::<char>(),
    ]
}

fn interesting_string() -> impl Strategy<Value = String> {
    prop::collection::vec(interesting_char(), 0..16)
        .prop_map(|chars| chars.into_iter().collect::<String>())
}

fn any_action() -> impl Strategy<Value = InputAction> {
    prop_oneof![
        3 => interesting_char().prop_map(InputAction::Insert),
        3 => Just(InputAction::DeleteWord),
        3 => Just(InputAction::WordLeft),
        3 => Just(InputAction::WordRight),
        2 => Just(InputAction::Backspace),
        2 => Just(InputAction::Delete),
        2 => Just(InputAction::Left),
        2 => Just(InputAction::Right),
        1 => Just(InputAction::Home),
        1 => Just(InputAction::End),
        1 => Just(InputAction::Clear),
        1 => Just(InputAction::Submit),
        1 => Just(InputAction::HistoryPrev),
        1 => Just(InputAction::HistoryNext),
        1 => Just(InputAction::Complete),
        1 => Just(InputAction::None),
    ]
}

/// The invariant every action must preserve: the cursor is a byte offset into
/// `content` that lands on a character boundary. Violating it does not fail
/// here — it detonates later, wherever the buffer next slices itself.
fn assert_cursor_is_valid(buf: &InputBuffer, context: &str) -> Result<(), TestCaseError> {
    prop_assert!(
        buf.cursor() <= buf.content().len(),
        "{context}: cursor {} past end of {:?} (len {})",
        buf.cursor(),
        buf.content(),
        buf.content().len()
    );
    prop_assert!(
        buf.content().is_char_boundary(buf.cursor()),
        "{context}: cursor {} is not a char boundary of {:?}",
        buf.cursor(),
        buf.content()
    );
    Ok(())
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(default_cases().max(256)))]

    /// Any action sequence applied to any starting content keeps the cursor a
    /// valid char boundary — and does not panic getting there.
    #[test]
    fn cursor_stays_on_a_char_boundary_through_any_action_sequence(
        content in interesting_string(),
        actions in prop::collection::vec(any_action(), 1..24),
    ) {
        let mut buf = InputBuffer::new();
        buf.set_content(content.clone());
        assert_cursor_is_valid(&buf, "after set_content")?;

        for (i, action) in actions.into_iter().enumerate() {
            let before = buf.content().to_string();
            buf.handle(action.clone());
            assert_cursor_is_valid(
                &buf,
                &format!("after action {i} {action:?} on {before:?}"),
            )?;
        }
    }

    /// The same invariant when the content is built keystroke by keystroke
    /// rather than pasted, which is the path history and `Submit` take.
    #[test]
    fn cursor_stays_on_a_char_boundary_when_typed_from_empty(
        actions in prop::collection::vec(any_action(), 1..32),
    ) {
        let mut buf = InputBuffer::new();

        for (i, action) in actions.into_iter().enumerate() {
            let before = buf.content().to_string();
            buf.handle(action.clone());
            assert_cursor_is_valid(
                &buf,
                &format!("after action {i} {action:?} on {before:?}"),
            )?;
        }
    }

    /// Boundary safety alone would be satisfied by a cursor that never moves,
    /// so pin the semantics too: `WordLeft` lands at the start of the line or
    /// immediately after a whitespace character — never in its middle, and
    /// never one byte into it.
    #[test]
    fn word_left_lands_at_the_start_of_a_word(
        content in interesting_string(),
        steps in 1usize..6,
    ) {
        let mut buf = InputBuffer::new();
        buf.set_content(content);

        for _ in 0..steps {
            buf.handle(InputAction::WordLeft);
            assert_cursor_is_valid(&buf, "after WordLeft")?;

            let preceding = buf.content()[..buf.cursor()].chars().next_back();
            prop_assert!(
                buf.cursor() == 0 || preceding.is_some_and(char::is_whitespace),
                "WordLeft stopped at {} in {:?}, preceded by {preceding:?}",
                buf.cursor(),
                buf.content()
            );
        }
    }

    /// `DeleteWord` removes only the word before the cursor: everything from
    /// the old cursor onward survives untouched, and the new cursor sits where
    /// the deleted word began.
    #[test]
    fn delete_word_keeps_the_text_after_the_cursor(
        content in interesting_string(),
        cursor_steps in 0usize..8,
    ) {
        let mut buf = InputBuffer::new();
        buf.set_content(content);
        for _ in 0..cursor_steps {
            buf.handle(InputAction::Left);
        }

        let before = buf.content().to_string();
        let cursor_before = buf.cursor();
        buf.handle(InputAction::DeleteWord);
        assert_cursor_is_valid(&buf, "after DeleteWord")?;

        prop_assert_eq!(
            buf.content(),
            format!("{}{}", &before[..buf.cursor()], &before[cursor_before..]),
            "DeleteWord disturbed text outside the deleted word"
        );
        prop_assert!(buf.cursor() <= cursor_before, "DeleteWord moved the cursor right");
    }
}
