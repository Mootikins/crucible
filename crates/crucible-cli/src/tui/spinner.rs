//! Throttled spinner animation for status indicators.
//!
//! Provides a consistent animation rate regardless of how frequently
//! the status is updated (e.g., on every token during streaming).

use std::time::{Duration, Instant};

/// Spinner animation frames (quarter-circle rotation).
pub const FRAMES: &[&str] = &["◐", "◓", "◑", "◒"];

/// Default throttle interval for spinner animation.
pub const DEFAULT_THROTTLE_MS: u64 = 500;

/// A throttled spinner that only advances frames at a fixed interval.
///
/// This prevents the spinner from updating too rapidly during fast
/// streaming, making it easier to visually track.
#[derive(Debug, Clone)]
pub struct Spinner {
    /// Current frame index (0..FRAMES.len())
    frame: usize,
    /// Last time the frame was advanced
    last_tick: Instant,
    /// Minimum time between frame advances
    throttle: Duration,
}

impl Default for Spinner {
    fn default() -> Self {
        Self::new()
    }
}

impl Spinner {
    /// Create a new spinner with default 500ms throttle.
    pub fn new() -> Self {
        Self {
            frame: 0,
            last_tick: Instant::now(),
            throttle: Duration::from_millis(DEFAULT_THROTTLE_MS),
        }
    }

    /// Create a spinner with custom throttle interval.
    pub fn with_throttle_ms(ms: u64) -> Self {
        Self {
            frame: 0,
            last_tick: Instant::now(),
            throttle: Duration::from_millis(ms),
        }
    }

    /// Tick the spinner, advancing the frame if throttle has elapsed.
    ///
    /// Call this on every update (e.g., every token), but the frame
    /// will only advance if enough time has passed.
    ///
    /// Returns true if the frame actually changed.
    pub fn tick(&mut self) -> bool {
        let now = Instant::now();
        if now.duration_since(self.last_tick) >= self.throttle {
            self.frame = (self.frame + 1) % FRAMES.len();
            self.last_tick = now;
            true
        } else {
            false
        }
    }

    /// Get the current frame index.
    pub fn frame(&self) -> usize {
        self.frame
    }

    /// Get the current spinner character.
    pub fn char(&self) -> &'static str {
        FRAMES[self.frame]
    }

    /// Reset the spinner to initial state.
    pub fn reset(&mut self) {
        self.frame = 0;
        self.last_tick = Instant::now();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread::sleep;

    #[test]
    fn test_spinner_starts_at_frame_zero() {
        let spinner = Spinner::new();
        assert_eq!(spinner.frame(), 0);
        assert_eq!(spinner.char(), "◐");
    }

    #[test]
    fn test_spinner_throttles() {
        let mut spinner = Spinner::with_throttle_ms(50);

        // First tick shouldn't advance (just created)
        assert!(!spinner.tick());
        assert_eq!(spinner.frame(), 0);

        // Wait for throttle
        sleep(Duration::from_millis(60));

        // Now it should advance
        assert!(spinner.tick());
        assert_eq!(spinner.frame(), 1);
    }

    #[test]
    fn test_spinner_wraps() {
        let mut spinner = Spinner::with_throttle_ms(1);

        // Advance through all frames
        for expected in 1..=FRAMES.len() {
            sleep(Duration::from_millis(2));
            spinner.tick();
            assert_eq!(spinner.frame(), expected % FRAMES.len());
        }
    }

    #[test]
    fn test_spinner_reset() {
        let mut spinner = Spinner::with_throttle_ms(1);
        sleep(Duration::from_millis(2));
        spinner.tick();
        assert_eq!(spinner.frame(), 1);

        spinner.reset();
        assert_eq!(spinner.frame(), 0);
    }
}
