//! UI constants for the TUI
//!
//! This module centralizes magic numbers used throughout the TUI codebase.
//! By defining constants in one place, we ensure consistency and make it easier
//! to adjust the UI layout.

use ratatui::layout::Rect;

/// Content margin for conversation text
///
/// This accounts for:
/// - Prefix characters (e.g., " ● " or " > " = 3 chars)
/// - Right margin (1 char)
pub const CONTENT_MARGIN: usize = 4;

/// Dialog padding (border width on each side)
///
/// This accounts for:
/// - Left border (1 char)
/// - Right border (1 char)
pub const DIALOG_PADDING: usize = 2;

/// Border height for input dialogs
///
/// Used for vertical centering calculations in input dialogs.
pub const DIALOG_BORDER_HEIGHT: u16 = 3;

/// Border size (one border line)
///
/// Used for subtracting top/bottom or left/right borders from area dimensions.
pub const BORDER_SIZE: u16 = 1;

/// Total border height (top + bottom)
///
/// Used for subtracting both top and bottom borders from area height.
pub const BORDER_HEIGHT_TOTAL: u16 = 2;

/// Default button width in dialogs
pub const BUTTON_WIDTH: u16 = 10;

/// Gap between buttons in dialogs
pub const BUTTON_GAP: u16 = 4;

/// UI constant helper functions
pub struct UiConstants;

impl UiConstants {
    /// Calculate content width from area width
    ///
    /// Subtracts the content margin (prefix + right margin) from the total width.
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::constants::UiConstants;
    ///
    /// let content_width = UiConstants::content_width(80);
    /// assert_eq!(content_width, 76);
    /// ```
    #[inline]
    pub const fn content_width(area_width: u16) -> usize {
        (area_width as usize).saturating_sub(CONTENT_MARGIN)
    }

    /// Calculate dialog width from outer area width
    ///
    /// Subtracts the dialog padding (borders) from the total width.
    ///
    /// # Examples
    /// ```
    /// use crucible_cli::tui::constants::UiConstants;
    ///
    /// let dialog_width = UiConstants::dialog_width(50);
    /// assert_eq!(dialog_width, 48);
    /// ```
    #[inline]
    pub const fn dialog_width(outer_width: u16) -> u16 {
        outer_width.saturating_sub(DIALOG_PADDING as u16)
    }

    /// Calculate maximum text width in a dialog
    ///
    /// Subtracts padding from both sides for text content.
    #[inline]
    pub const fn dialog_text_width(outer_width: u16) -> u16 {
        outer_width.saturating_sub((DIALOG_PADDING * 2) as u16)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_content_width() {
        assert_eq!(UiConstants::content_width(80), 76);
        assert_eq!(UiConstants::content_width(10), 6);
        assert_eq!(UiConstants::content_width(4), 0); // saturating_sub
        assert_eq!(UiConstants::content_width(2), 0); // saturating_sub
    }

    #[test]
    fn test_dialog_width() {
        assert_eq!(UiConstants::dialog_width(50), 48);
        assert_eq!(UiConstants::dialog_width(10), 8);
        assert_eq!(UiConstants::dialog_width(2), 0); // saturating_sub
    }

    #[test]
    fn test_dialog_text_width() {
        assert_eq!(UiConstants::dialog_text_width(50), 46);
        assert_eq!(UiConstants::dialog_text_width(10), 6);
        assert_eq!(UiConstants::dialog_text_width(4), 0); // saturating_sub
    }
}
