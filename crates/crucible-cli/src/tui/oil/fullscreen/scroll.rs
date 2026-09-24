//! The scroll position of a full-screen buffer.
//!
//! `top` is the first buffer row on screen. While `follow` is on, the view
//! stays at the bottom as rows arrive. A manual scroll up turns follow off; a
//! scroll that reaches the bottom turns it on again. Nothing else turns it
//! on: a reflow or a shrink that shows the bottom keeps a reader's place for
//! the next reflow.

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Scroll {
    top: usize,
    follow: bool,
}

impl Default for Scroll {
    fn default() -> Self {
        Self {
            top: 0,
            follow: true,
        }
    }
}

impl Scroll {
    pub fn top(&self) -> usize {
        self.top
    }

    pub fn follows(&self) -> bool {
        self.follow
    }

    /// Fit the position to a buffer of `total` rows in a `height`-row view.
    /// Call this once per frame, after the buffer changed.
    pub fn fit(&mut self, total: usize, height: usize) {
        let bottom = total.saturating_sub(height);
        if self.follow || self.top > bottom {
            self.top = bottom;
        }
    }

    /// Scroll by `delta` rows; negative scrolls toward the start.
    pub fn scroll_by(&mut self, delta: isize, total: usize, height: usize) {
        let bottom = total.saturating_sub(height);
        self.top = self.top.saturating_add_signed(delta).min(bottom);
        self.follow = self.top >= bottom;
    }

    /// Put `row` at the top of the view, as a reflow does to keep the reader
    /// at the same text. Follow stays off, even when the row is in the last
    /// page, so the next reflow still knows the reader's place.
    pub fn set_top(&mut self, row: usize, total: usize, height: usize) {
        self.top = row.min(total.saturating_sub(height));
        self.follow = false;
    }

    /// Go to the bottom and follow new rows.
    pub fn follow_bottom(&mut self, total: usize, height: usize) {
        self.top = total.saturating_sub(height);
        self.follow = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_new_scroll_follows_the_bottom_as_rows_arrive() {
        let mut scroll = Scroll::default();
        scroll.fit(100, 10);
        assert_eq!(scroll.top(), 90);
        scroll.fit(150, 10);
        assert_eq!(scroll.top(), 140);
        assert!(scroll.follows());
    }

    #[test]
    fn a_scroll_up_turns_follow_off_and_holds_the_rows() {
        let mut scroll = Scroll::default();
        scroll.fit(100, 10);
        scroll.scroll_by(-5, 100, 10);
        assert!(!scroll.follows());
        assert_eq!(scroll.top(), 85);

        scroll.fit(150, 10);
        assert_eq!(scroll.top(), 85, "new rows must not move a reader");
    }

    #[test]
    fn a_scroll_that_reaches_the_bottom_turns_follow_on() {
        let mut scroll = Scroll::default();
        scroll.fit(100, 10);
        scroll.scroll_by(-5, 100, 10);
        scroll.scroll_by(50, 100, 10);
        assert!(scroll.follows());
        assert_eq!(scroll.top(), 90);
    }

    #[test]
    fn a_scroll_stops_at_the_start() {
        let mut scroll = Scroll::default();
        scroll.fit(100, 10);
        scroll.scroll_by(-500, 100, 10);
        assert_eq!(scroll.top(), 0);
        assert!(!scroll.follows());
    }

    #[test]
    fn a_short_buffer_follows_after_any_scroll() {
        let mut scroll = Scroll::default();
        scroll.scroll_by(-3, 5, 10);
        assert_eq!(scroll.top(), 0);
        assert!(
            scroll.follows(),
            "a buffer shorter than the view is at its bottom"
        );
    }

    #[test]
    fn a_shrunk_buffer_pulls_the_reader_to_its_bottom_without_following() {
        let mut scroll = Scroll::default();
        scroll.fit(100, 10);
        scroll.scroll_by(-20, 100, 10);
        scroll.fit(50, 10);
        assert_eq!(scroll.top(), 40);
        assert!(!scroll.follows(), "only a scroll turns follow on");
    }
}
