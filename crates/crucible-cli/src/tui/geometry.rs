//! Geometry helpers for UI layout calculations
//!
//! This module provides reusable functions for common UI layout operations
//! such as centering content and calculating positions.

use ratatui::layout::Rect;

/// Geometry helper functions for UI layout
pub struct PopupGeometry;

impl PopupGeometry {
    /// Calculate horizontal center position for content
    ///
    /// Returns the x-coordinate that centers content of the given width
    /// within the containing area.
    ///
    /// # Arguments
    /// * `inner` - The containing area
    /// * `content_width` - The width of the content to center
    ///
    /// # Examples
    /// ```
    /// use ratatui::layout::Rect;
    /// use crucible_cli::tui::geometry::PopupGeometry;
    ///
    /// let area = Rect::new(0, 0, 80, 24);
    /// let x = PopupGeometry::center_horizontally(area, 20);
    /// assert_eq!(x, 30); // (80 - 20) / 2 = 30
    /// ```
    #[inline]
    pub const fn center_horizontally(inner: Rect, content_width: u16) -> u16 {
        inner.x + inner.width.saturating_sub(content_width) / 2
    }

    /// Calculate vertical center position for content
    ///
    /// Returns the y-coordinate that centers content of the given height
    /// within the containing area.
    ///
    /// # Arguments
    /// * `inner` - The containing area
    /// * `content_height` - The height of the content to center
    ///
    /// # Examples
    /// ```
    /// use ratatui::layout::Rect;
    /// use crucible_cli::tui::geometry::PopupGeometry;
    ///
    /// let area = Rect::new(0, 0, 80, 24);
    /// let y = PopupGeometry::center_vertically(area, 10);
    /// assert_eq!(y, 7); // 0 + (24 - 10) / 2 = 7
    /// ```
    #[inline]
    pub const fn center_vertically(inner: Rect, content_height: u16) -> u16 {
        inner.y + inner.height.saturating_sub(content_height) / 2
    }

    /// Calculate vertical center position for content when content is smaller
    ///
    /// Returns the y-coordinate that centers content vertically within the area.
    /// If content height is greater than or equal to area height, returns the
    /// area's y-coordinate (top alignment).
    ///
    /// # Arguments
    /// * `area` - The containing area
    /// * `content_height` - The height of the content to center
    ///
    /// # Examples
    /// ```
    /// use ratatui::layout::Rect;
    /// use crucible_cli::tui::geometry::PopupGeometry;
    ///
    /// let area = Rect::new(0, 0, 80, 24);
    ///
    /// // Content fits - center it
    /// let y = PopupGeometry::center_vertically_if_fits(area, 10);
    /// assert_eq!(y, 7);
    ///
    /// // Content too tall - top align
    /// let y = PopupGeometry::center_vertically_if_fits(area, 30);
    /// assert_eq!(y, 0);
    /// ```
    #[inline]
    pub fn center_vertically_if_fits(area: Rect, content_height: u16) -> u16 {
        if content_height < area.height {
            area.y + (area.height - content_height) / 2
        } else {
            area.y
        }
    }

    /// Calculate horizontal center position for text
    ///
    /// Returns the x-coordinate that centers text of the given length
    /// within the containing area.
    ///
    /// # Arguments
    /// * `inner` - The containing area
    /// * `text_len` - The length of the text to center (as u16)
    ///
    /// # Examples
    /// ```
    /// use ratatui::layout::Rect;
    /// use crucible_cli::tui::geometry::PopupGeometry;
    ///
    /// let area = Rect::new(0, 0, 80, 24);
    /// let x = PopupGeometry::center_text_horizontally(area, 12);
    /// assert_eq!(x, 34); // 0 + (80 - 12) / 2 = 34
    /// ```
    #[inline]
    pub const fn center_text_horizontally(inner: Rect, text_len: u16) -> u16 {
        inner.x + inner.width.saturating_sub(text_len) / 2
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_center_horizontally() {
        let area = Rect::new(10, 5, 80, 24);

        // Perfect center
        assert_eq!(PopupGeometry::center_horizontally(area, 40), 30); // 10 + (80-40)/2 = 30

        // Odd width
        assert_eq!(PopupGeometry::center_horizontally(area, 21), 39); // 10 + (80-21)/2 = 39

        // Content wider than area
        assert_eq!(PopupGeometry::center_horizontally(area, 100), 10); // 10 + (80-100)/2 = 10
    }

    #[test]
    fn test_center_vertically() {
        let area = Rect::new(10, 5, 80, 24);

        // Perfect center
        assert_eq!(PopupGeometry::center_vertically(area, 12), 11); // 5 + (24-12)/2 = 11

        // Odd height
        assert_eq!(PopupGeometry::center_vertically(area, 11), 11); // 5 + (24-11)/2 = 11

        // Content taller than area
        assert_eq!(PopupGeometry::center_vertically(area, 30), 5); // 5 + (24-30)/2 = 5
    }

    #[test]
    fn test_center_vertically_if_fits() {
        let area = Rect::new(0, 0, 80, 24);

        // Content fits - center it
        assert_eq!(PopupGeometry::center_vertically_if_fits(area, 10), 7);

        // Content exactly fits
        assert_eq!(PopupGeometry::center_vertically_if_fits(area, 24), 0);

        // Content too tall - top align
        assert_eq!(PopupGeometry::center_vertically_if_fits(area, 30), 0);
    }

    #[test]
    fn test_center_text_horizontally() {
        let area = Rect::new(10, 5, 80, 24);

        // Short text
        assert_eq!(PopupGeometry::center_text_horizontally(area, 12), 44); // 10 + (80-12)/2 = 44

        // Long text
        assert_eq!(PopupGeometry::center_text_horizontally(area, 70), 15); // 10 + (80-70)/2 = 15

        // Text too wide
        assert_eq!(PopupGeometry::center_text_horizontally(area, 100), 10); // 10 + (80-100)/2 = 10 (saturating_sub)
    }
}
