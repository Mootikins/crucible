//! Decrypt text animation effect
//!
//! Provides a "movie-style" text decryption animation where characters
//! start as scrambled cipher characters and progressively reveal their
//! final form. Commonly used during streaming to create visual interest
//! while text is being generated.
//!
//! # Example
//!
//! ```rust,ignore
//! use crucible_oil::decrypt::decrypt_text;
//!
//! // Show "Hello" with first 2 chars revealed, rest scrambled
//! let node = decrypt_text("Hello", 2, frame_counter);
//! ```

use crate::node::{row, styled, text, Node};
use crate::style::{Color, Style};

/// Character set used for scrambled cipher display.
///
/// Includes box-drawing characters, symbols, and numbers to create
/// a "hacker movie" aesthetic during the scramble animation.
pub const CIPHER_CHARS: &[char] = &[
    '▓', '░', '▒', '█', '▄', '▀', '│', '─', '┐', '└', '┘', '┌', '╱', '╲', '╳', '○', '●', '◐', '◑',
    '◒', '◓', '!', '@', '#', '$', '%', '^', '&', '*', '0', '1', '2', '3', '4', '5', '6', '7', '8',
    '9',
];

/// Render text with a movie-style decrypt animation effect.
///
/// Characters before `revealed_count` show their final form (plaintext).
/// Characters at or after `revealed_count` show scrambled cipher characters
/// that change based on position and the animation frame.
///
/// # Arguments
///
/// * `content` - The final text to reveal
/// * `revealed_count` - How many characters (from the start) are fully revealed
/// * `frame` - Animation frame counter (cycles cipher characters for scramble effect)
///
/// # Returns
///
/// A `Node` that renders the text with revealed and scrambled portions.
/// If all characters are revealed, returns a simple text node for efficiency.
///
/// # Example
///
/// ```rust,ignore
/// // Frame 0: "He░▓▒"
/// // Frame 1: "He▒█▄"
/// // Frame 2: "He▀│─" (scramble portion changes each frame)
/// // After settle: "Hello"
/// let node = decrypt_text("Hello", 2, frame);
/// ```
pub fn decrypt_text(content: &str, revealed_count: usize, frame: usize) -> Node {
    DecryptConfig::default().render(content, revealed_count, frame)
}

/// Configuration for decrypt animation behavior.
#[derive(Debug, Clone)]
pub struct DecryptConfig {
    /// Characters to use for the scramble effect
    pub cipher_chars: Vec<char>,
    /// Style for revealed (plaintext) characters
    pub revealed_style: Style,
    /// Style for cipher (scrambled) characters
    pub cipher_style: Style,
}

impl Default for DecryptConfig {
    fn default() -> Self {
        Self {
            cipher_chars: CIPHER_CHARS.to_vec(),
            revealed_style: Style::default(),
            cipher_style: Style::new().fg(Color::Green).dim(),
        }
    }
}

impl DecryptConfig {
    /// Create a config with custom cipher characters
    pub fn with_cipher_chars(mut self, chars: impl Into<Vec<char>>) -> Self {
        self.cipher_chars = chars.into();
        self
    }

    /// Set the style for revealed (plaintext) characters
    pub fn with_revealed_style(mut self, style: Style) -> Self {
        self.revealed_style = style;
        self
    }

    /// Set the style for cipher characters
    pub fn with_cipher_style(mut self, style: Style) -> Self {
        self.cipher_style = style;
        self
    }

    /// Render text with this config's settings
    pub fn render(&self, content: &str, revealed_count: usize, frame: usize) -> Node {
        let chars: Vec<char> = content.chars().collect();

        // Fast path: empty content
        if chars.is_empty() {
            return Node::Empty;
        }

        // Fast path: fully revealed
        if revealed_count >= chars.len() {
            return text(content.to_string());
        }

        let mut children = Vec::with_capacity(chars.len());

        for (i, ch) in chars.iter().enumerate() {
            if i < revealed_count {
                if self.revealed_style == Style::default() {
                    children.push(text(ch.to_string()));
                } else {
                    children.push(styled(ch.to_string(), self.revealed_style));
                }
            } else {
                let idx = (i.wrapping_mul(7).wrapping_add(frame.wrapping_mul(3)))
                    % self.cipher_chars.len();
                children.push(styled(
                    self.cipher_chars[idx].to_string(),
                    self.cipher_style,
                ));
            }
        }

        row(children)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::render::render_to_string;

    #[test]
    fn fully_revealed_returns_plain_text() {
        let node = decrypt_text("hello", 10, 0);
        assert!(matches!(node, Node::Text(_)));
    }

    #[test]
    fn partial_reveal_returns_row() {
        let node = decrypt_text("hello", 2, 0);
        assert!(matches!(node, Node::Box(_)));
    }

    #[test]
    fn empty_content_returns_empty() {
        let node = decrypt_text("", 0, 0);
        assert!(matches!(node, Node::Empty));
    }

    #[test]
    fn zero_revealed_all_scrambled() {
        let node = decrypt_text("test", 0, 0);
        let output = render_to_string(&node, 80);
        // Should not contain any of the original characters
        assert!(!output.contains('t'));
        assert!(!output.contains('e'));
        assert!(!output.contains('s'));
    }

    #[test]
    fn different_frames_produce_different_cipher() {
        let frame0 = render_to_string(&decrypt_text("test", 0, 0), 80);
        let frame1 = render_to_string(&decrypt_text("test", 0, 1), 80);

        // Different frames should produce different scrambled output
        // (stripping ANSI to compare actual characters)
        let frame0_stripped = crate::utils::strip_ansi(&frame0);
        let frame1_stripped = crate::utils::strip_ansi(&frame1);
        assert_ne!(frame0_stripped, frame1_stripped);
    }

    #[test]
    fn revealed_chars_appear_in_output() {
        let node = decrypt_text("hello", 3, 0);
        let output = render_to_string(&node, 80);
        let stripped = crate::utils::strip_ansi(&output);
        // First 3 chars should be revealed
        assert!(stripped.starts_with("hel"));
    }

    #[test]
    fn config_custom_cipher_chars() {
        let config = DecryptConfig::default().with_cipher_chars(vec!['X', 'Y', 'Z']);

        let node = config.render("test", 0, 0);
        let output = render_to_string(&node, 80);
        let stripped = crate::utils::strip_ansi(&output);

        // Should only contain X, Y, or Z
        assert!(stripped.chars().all(|c| c == 'X' || c == 'Y' || c == 'Z'));
    }

    #[test]
    fn progressive_reveal() {
        // Simulate progressive reveal
        for revealed in 0..=5 {
            let node = decrypt_text("hello", revealed, 0);
            let output = render_to_string(&node, 80);
            let stripped = crate::utils::strip_ansi(&output);

            // Check that correct number of chars are revealed
            let hello_chars: Vec<char> = "hello".chars().collect();
            for (i, expected) in hello_chars.iter().enumerate() {
                if i < revealed {
                    // This position should show the real character
                    let actual = stripped.chars().nth(i);
                    assert_eq!(actual, Some(*expected), "Position {} should be revealed", i);
                }
            }
        }
    }
}
