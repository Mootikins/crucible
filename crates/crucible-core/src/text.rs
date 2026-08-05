//! Making untrusted text safe to paint into a terminal.
//!
//! Text that reaches a Crucible surface is not all authored by the user.
//! Branch names, shell output, model prose and — widest of all — the stdout of
//! a delegated ACP agent process all end up rendered, and none of them are
//! trusted to do more than *read* as characters.
//!
//! Sanitising belongs at the ingest boundary, once per source, rather than at
//! each renderer: the same string is painted by the TUI, persisted to the
//! session log, replayed into a recording and served to the web view, and only
//! the boundary sees all four.

/// Whether a character must not survive into rendered text.
///
/// Control characters are the obvious half. They move the cursor, ring the
/// bell, or open an escape sequence, and nothing that arrives as *data* has a
/// legitimate reason to do any of that.
///
/// The bidi and zero-width formatting characters are the less obvious half.
/// They are not control characters, so `is_control` misses them, but a
/// right-to-left override reorders how the rest of the line *reads* without
/// changing what it contains — the display-spoofing trick behind filenames
/// like `annexe\u{202E}cod.exe`.
///
/// C1 (`\u{80}`–`\u{9F}`) is covered by `is_control`, and covering it matters:
/// those are ordinary `char`s to `unicode_width` and to every ESC-sequence
/// filter that scans for `\x1b`, but xterm and its derivatives decode the
/// UTF-8 encoding of `\u{9B}` back into CSI — so `\u{9B}2J` clears the screen
/// through a filter that only ever looked for the 7-bit form.
pub fn is_display_hostile(c: char) -> bool {
    c.is_control()
        || matches!(c,
            // LRM, RLM, ALM
            '\u{200E}' | '\u{200F}' | '\u{061C}'
            // embeddings and overrides: LRE, RLE, PDF, LRO, RLO
            | '\u{202A}'..='\u{202E}'
            // isolates: LRI, RLI, FSI, PDI
            | '\u{2066}'..='\u{2069}'
            // zero-width space / non-joiner / joiner / no-break space
            | '\u{200B}'..='\u{200D}' | '\u{FEFF}')
}

/// Strip every hostile character, newlines and tabs included.
///
/// For text that occupies a single line by construction — a status bar
/// segment, a tool title, a one-line label. A newline there is as much a
/// layout escape as a cursor movement is.
pub fn sanitize_single_line(value: &str) -> String {
    value.chars().filter(|c| !is_display_hostile(*c)).collect()
}

/// Strip every hostile character *except* `\n` and `\t`.
///
/// For prose — an agent's narration, its reasoning, an error sentence. Those
/// two carry meaning an author intended (paragraphs, indented code) and every
/// renderer downstream already treats them as layout rather than as terminal
/// commands. `\r` is deliberately not in the exception set: it rewinds the
/// cursor to the start of the line, which overwrites what was already printed.
pub fn sanitize_multiline(value: &str) -> String {
    value
        .chars()
        .filter(|c| matches!(c, '\n' | '\t') || !is_display_hostile(*c))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn seven_bit_escape_sequences_do_not_survive() {
        assert_eq!(
            sanitize_multiline("main\x1b[2J\x1b]0;pwned\x07"),
            "main[2J]0;pwned"
        );
    }

    /// The C1 bypass: `\u{9B}` is not `\x1b`, so an ESC-scanning filter passes
    /// it through, and a terminal in UTF-8 mode decodes it back to CSI.
    #[test]
    fn eight_bit_c1_introducers_do_not_survive() {
        for c1 in ['\u{9B}', '\u{9D}', '\u{90}', '\u{9F}'] {
            assert!(
                is_display_hostile(c1),
                "{c1:?} is a C1 introducer and must be stripped"
            );
        }
        assert_eq!(sanitize_multiline("safe\u{9B}2J"), "safe2J");
    }

    #[test]
    fn bidi_and_zero_width_characters_do_not_survive() {
        let cleaned = sanitize_multiline("annexe\u{202E}cod.exe\u{200B}\u{2066}x");
        assert_eq!(cleaned, "annexecod.exex");
    }

    /// The whole reason there are two functions: prose keeps its layout, a
    /// single-line surface keeps none.
    #[test]
    fn only_the_multiline_form_keeps_newlines_and_tabs() {
        let prose = "para one\n\nindented:\n\tcode";
        assert_eq!(sanitize_multiline(prose), prose);
        assert_eq!(
            sanitize_single_line(prose),
            "para oneindented:code",
            "a single-line surface must not be escapable by a newline"
        );
    }

    /// `\r` reads like layout but behaves like a cursor command: it rewinds to
    /// column zero, so the next characters overwrite what was already painted.
    #[test]
    fn carriage_return_is_not_treated_as_layout() {
        assert_eq!(
            sanitize_multiline("real answer\rfake answer"),
            "real answerfake answer"
        );
    }

    #[test]
    fn ordinary_text_is_untouched() {
        let text = "Reading `src/main.rs` — 你好 🦀 (100%)";
        assert_eq!(sanitize_multiline(text), text);
        assert_eq!(sanitize_single_line(text), text);
    }
}
