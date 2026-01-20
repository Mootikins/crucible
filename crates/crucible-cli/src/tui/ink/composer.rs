//! Composer region: fixed-height bottom area for input, status, and popup.
//!
//! The composer always occupies the same vertical space regardless of whether
//! the popup is visible. This eliminates viewport "shake" when toggling popups.
//!
//! ## Layout Budget
//!
//! ```text
//! ┌─────────────────────────┐
//! │ top_edge (1 line)       │  ─┐
//! │ input lines (N lines)   │   ├─ input_height (variable, clamped)
//! │ bottom_edge (1 line)    │  ─┘
//! ├─────────────────────────┤
//! │ status bar (1 line)     │  ← status_height (always 1)
//! ├─────────────────────────┤
//! │ popup (visible)         │  ─┐
//! │   or                    │   ├─ popup_height (fixed, blank when hidden)
//! │ blank lines (hidden)    │  ─┘
//! └─────────────────────────┘
//! ```
//!
//! Total height = input_height + status_height + popup_height

use super::viewport::pad_lines_to;

/// Configuration for the composer region's height budget.
#[derive(Debug, Clone)]
pub struct ComposerConfig {
    /// Height budget for input box (including borders).
    /// Input content will be clamped/scrolled within this budget.
    pub input_height: usize,

    /// Height for status bar (typically 1).
    pub status_height: usize,

    /// Height reserved for popup (rendered as blank when hidden).
    pub popup_height: usize,
}

impl Default for ComposerConfig {
    fn default() -> Self {
        Self {
            input_height: 5,
            status_height: 1,
            popup_height: 10,
        }
    }
}

impl ComposerConfig {
    /// Total height of the composer region.
    pub fn total_height(&self) -> usize {
        self.input_height + self.status_height + self.popup_height
    }

    /// Create a config with custom heights.
    pub fn new(input_height: usize, status_height: usize, popup_height: usize) -> Self {
        Self {
            input_height,
            status_height,
            popup_height,
        }
    }
}

/// Pad popup content to exactly `popup_height` lines.
///
/// When the popup is hidden, pass an empty slice to get blank lines.
/// When visible, the popup items are padded/clamped to the fixed height.
///
/// This ensures the composer region always has stable vertical size.
pub fn pad_popup_region(lines: &[String], popup_height: usize) -> Vec<String> {
    let mut result = lines.to_vec();
    pad_lines_to(&mut result, popup_height);
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_config_has_reasonable_values() {
        let config = ComposerConfig::default();
        assert_eq!(config.input_height, 5);
        assert_eq!(config.status_height, 1);
        assert_eq!(config.popup_height, 10);
        assert_eq!(config.total_height(), 16);
    }

    #[test]
    fn custom_config_calculates_total() {
        let config = ComposerConfig::new(3, 1, 8);
        assert_eq!(config.total_height(), 12);
    }

    #[test]
    fn pad_popup_region_empty_produces_blank_lines() {
        let result = pad_popup_region(&[], 5);
        assert_eq!(result.len(), 5);
        assert!(result.iter().all(|s| s.is_empty()));
    }

    #[test]
    fn pad_popup_region_under_height_pads() {
        let lines = vec!["item1".to_string(), "item2".to_string()];
        let result = pad_popup_region(&lines, 5);
        assert_eq!(result.len(), 5);
        assert_eq!(result[0], "item1");
        assert_eq!(result[1], "item2");
        assert_eq!(result[2], "");
        assert_eq!(result[3], "");
        assert_eq!(result[4], "");
    }

    #[test]
    fn pad_popup_region_over_height_truncates_top() {
        let lines = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        let result = pad_popup_region(&lines, 2);
        assert_eq!(result.len(), 2);
        assert_eq!(result[0], "c");
        assert_eq!(result[1], "d");
    }

    #[test]
    fn pad_popup_region_exact_height_unchanged() {
        let lines = vec!["x".to_string(), "y".to_string(), "z".to_string()];
        let result = pad_popup_region(&lines, 3);
        assert_eq!(result, vec!["x", "y", "z"]);
    }
}
