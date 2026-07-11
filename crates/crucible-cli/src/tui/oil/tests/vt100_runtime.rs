//! vt100-backed test runtime — exercises the real terminal escape sequence path
//! and provides screen-level assertions through a virtual terminal emulator.
//!
//! Unlike TestRuntime which accumulates rendered strings, this feeds raw terminal
//! bytes through vt100::Parser and reads the actual screen state. This catches
//! bugs in cursor math, viewport clearing, and graduation that string-based
//! testing misses.

use crate::tui::oil::chat_app::OilChatApp;
use crate::tui::oil::chat_runner::render_frame;
use crucible_oil::focus::FocusContext;
use crucible_oil::TestRuntime;

/// Find a byte subsequence in a byte slice. Returns the start position.
fn find_subsequence(haystack: &[u8], needle: &[u8]) -> Option<usize> {
    haystack
        .windows(needle.len())
        .position(|window| window == needle)
}

/// A test runtime that feeds terminal output through vt100 for screen-level assertions.
pub struct Vt100TestRuntime {
    inner: TestRuntime,
    vt: vt100::Parser,
    /// Tall parser (1000 rows) that receives the same bytes. Since nothing
    /// scrolls off the top of a 1000-row terminal, `contents()` shows
    /// everything — equivalent to scrollback + screen.
    tall_vt: vt100::Parser,
    /// Raw bytes from the last render_frame call (captured before feeding to vt100).
    last_frame_bytes: Vec<u8>,
}

impl Vt100TestRuntime {
    pub fn new(width: u16, height: u16) -> Self {
        // Large scrollback to capture all graduated content
        let scrollback = 1000;
        Self {
            inner: TestRuntime::new(width, height),
            vt: vt100::Parser::new(height, width, scrollback),
            tall_vt: vt100::Parser::new(1000, width, 0),
            last_frame_bytes: Vec::new(),
        }
    }

    /// Render a frame through the real terminal path, then feed bytes to vt100.
    pub fn render_frame(&mut self, app: &mut OilChatApp) {
        let focus = FocusContext::new();
        render_frame(app, &mut self.inner, &focus);

        // Feed terminal bytes to vt100, respecting synchronized update
        // boundaries. Content inside sync blocks is fed atomically (as real
        // terminals buffer it). Content OUTSIDE sync blocks is fed
        // byte-by-byte (real terminals process it incrementally).
        let bytes = self.inner.take_bytes();
        self.last_frame_bytes = bytes.clone();
        if !bytes.is_empty() {
            self.feed_bytes_respecting_sync(&bytes);
        }
    }

    /// Feed bytes to vt100, processing synchronized update blocks atomically
    /// and non-sync content incrementally. This models real terminal behavior.
    /// Also feeds all bytes to the tall parser for scrollback inspection.
    fn feed_bytes_respecting_sync(&mut self, bytes: &[u8]) {
        // Tall parser gets all bytes (for scrollback content inspection)
        self.tall_vt.process(bytes);
        let begin = b"\x1b[?2026h";
        let end = b"\x1b[?2026l";

        let mut pos = 0;
        while pos < bytes.len() {
            // Look for BEGIN_SYNCHRONIZED_UPDATE
            if let Some(sync_start) = find_subsequence(&bytes[pos..], begin) {
                let abs_start = pos + sync_start;

                // Feed everything before the sync block incrementally
                if abs_start > pos {
                    for &b in &bytes[pos..abs_start] {
                        self.vt.process(&[b]);
                    }
                }

                // Find matching END_SYNCHRONIZED_UPDATE
                if let Some(sync_end) = find_subsequence(&bytes[abs_start..], end) {
                    let abs_end = abs_start + sync_end + end.len();
                    // Feed the entire sync block atomically
                    self.vt.process(&bytes[abs_start..abs_end]);
                    pos = abs_end;
                } else {
                    // No end marker — feed rest atomically
                    self.vt.process(&bytes[abs_start..]);
                    pos = bytes.len();
                }
            } else {
                // No more sync blocks — feed remaining incrementally
                for &b in &bytes[pos..] {
                    self.vt.process(&[b]);
                }
                pos = bytes.len();
            }
        }
    }

    /// Get the full screen contents (visible area) as plain text.
    pub fn screen_contents(&self) -> String {
        self.vt.screen().contents()
    }

    /// Get the screen contents with ANSI escape codes preserved.
    /// Use this for "raw" snapshot tests that verify styling (colors, bold, etc.).
    pub fn screen_contents_styled(&self) -> String {
        String::from_utf8_lossy(&self.vt.screen().contents_formatted()).into_owned()
    }

    /// Get the underlying TestRuntime for legacy API access.
    pub fn inner(&self) -> &TestRuntime {
        &self.inner
    }

    /// Read only the scrollback content (content that has scrolled off the top).
    ///
    /// Uses vt100's `set_scrollback()` to shift the viewport, then extracts
    /// only the scrollback rows (not the current visible screen). This is
    /// important because the visible screen may legitimately contain spinners
    /// (they're part of the active viewport), but scrollback should not.
    pub fn scrollback_contents(&mut self) -> String {
        // Use the tall parser — nothing scrolls off in a 1000-row terminal,
        // so contents() shows the full history. Extract the scrollback portion
        // by subtracting the normal parser's visible screen.
        let tall_contents = self.tall_vt.screen().contents();
        let screen_contents = self.vt.screen().contents();

        let tall_lines: Vec<&str> = tall_contents.lines().collect();
        let screen_lines: Vec<&str> = screen_contents.lines().collect();

        let scrollback_count = tall_lines.len().saturating_sub(screen_lines.len());
        if scrollback_count > 0 {
            tall_lines[..scrollback_count].join("\n")
        } else {
            String::new()
        }
    }

    /// Get the full history from the tall parser (scrollback + screen).
    /// Since the tall parser has 1000 rows, nothing scrolls off — this
    /// captures everything the terminal has ever displayed.
    pub fn full_history(&self) -> String {
        self.tall_vt.screen().contents()
    }

    /// Assert no spinner characters appear in scrollback content.
    ///
    /// Uses canonical spinner character sets from crucible_oil::node to avoid
    /// hardcoded character lists drifting out of sync.
    pub fn assert_no_spinners_in_scrollback(&mut self) {
        use crucible_oil::node::{BRAILLE_SPINNER_FRAMES, SPINNER_FRAMES};

        let contents = self.scrollback_contents();
        if contents.is_empty() {
            return;
        }

        for ch in SPINNER_FRAMES.iter().chain(BRAILLE_SPINNER_FRAMES.iter()) {
            assert!(
                !contents.contains(*ch),
                "Spinner char '{}' found in scrollback:\n{}",
                ch,
                contents
            );
        }
    }

    /// Get the raw bytes from the last render_frame call.
    ///
    /// Useful for verifying escape sequence structure like synchronized
    /// update boundaries.
    pub fn last_frame_bytes(&self) -> &[u8] {
        &self.last_frame_bytes
    }
}
