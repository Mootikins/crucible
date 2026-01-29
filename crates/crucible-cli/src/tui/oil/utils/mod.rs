//! Shared utility functions for TUI rendering
//!
//! This module consolidates commonly needed utilities for string manipulation,
//! terminal queries, and text layout. Use these canonical implementations
//! instead of creating local versions.
//!
//! # Modules
//!
//! - [`truncate`] - String truncation by width, characters, or lines
//! - [`wrap`] - Text wrapping with ANSI preservation
//! - [`width`] - Terminal dimensions and visible width calculation

pub mod truncate;
pub mod width;
pub mod wrap;

// Re-export commonly used items at the utils level for convenience
pub use truncate::{truncate_first_line, truncate_lines, truncate_to_chars, truncate_to_width};
pub use width::{cursor_position, terminal_height, terminal_size, terminal_width, visible_width};
pub use wrap::{wrap_to_width, wrap_to_width_indented};
