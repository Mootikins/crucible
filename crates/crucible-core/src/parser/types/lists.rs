//! List types for ordered and unordered lists with nesting support

use serde::{Deserialize, Serialize};

/// Extended checkbox status for task list items
///
/// Supports additional states beyond the basic pending/completed:
/// - `[ ]` (space) - Pending
/// - `[x]` or `[X]` - Done
/// - `[/]` - InProgress
/// - `[-]` - Cancelled
/// - `[!]` - Blocked
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum CheckboxStatus {
    /// Task is pending ([ ])
    Pending,
    /// Task is done ([x] or [X])
    Done,
    /// Task is in progress ([/])
    InProgress,
    /// Task is cancelled ([-])
    Cancelled,
    /// Task is blocked ([!])
    Blocked,
}

impl CheckboxStatus {
    /// Parse a checkbox status from a character
    pub fn from_char(c: char) -> Option<Self> {
        match c {
            ' ' => Some(CheckboxStatus::Pending),
            'x' | 'X' => Some(CheckboxStatus::Done),
            '/' => Some(CheckboxStatus::InProgress),
            '-' => Some(CheckboxStatus::Cancelled),
            '!' => Some(CheckboxStatus::Blocked),
            _ => None,
        }
    }

    /// Convert checkbox status to a character
    pub fn to_char(self) -> char {
        match self {
            CheckboxStatus::Pending => ' ',
            CheckboxStatus::Done => 'x',
            CheckboxStatus::InProgress => '/',
            CheckboxStatus::Cancelled => '-',
            CheckboxStatus::Blocked => '!',
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checkbox_status_from_char_space_is_pending() {
        assert_eq!(
            CheckboxStatus::from_char(' '),
            Some(CheckboxStatus::Pending)
        );
    }

    #[test]
    fn checkbox_status_from_char_x_is_done() {
        assert_eq!(CheckboxStatus::from_char('x'), Some(CheckboxStatus::Done));
        assert_eq!(CheckboxStatus::from_char('X'), Some(CheckboxStatus::Done));
    }

    #[test]
    fn checkbox_status_from_char_slash_is_in_progress() {
        assert_eq!(
            CheckboxStatus::from_char('/'),
            Some(CheckboxStatus::InProgress)
        );
    }

    #[test]
    fn checkbox_status_from_char_dash_is_cancelled() {
        assert_eq!(
            CheckboxStatus::from_char('-'),
            Some(CheckboxStatus::Cancelled)
        );
    }

    #[test]
    fn checkbox_status_from_char_bang_is_blocked() {
        assert_eq!(
            CheckboxStatus::from_char('!'),
            Some(CheckboxStatus::Blocked)
        );
    }

    #[test]
    fn checkbox_status_to_char_roundtrips() {
        let statuses = vec![
            CheckboxStatus::Pending,
            CheckboxStatus::Done,
            CheckboxStatus::InProgress,
            CheckboxStatus::Cancelled,
            CheckboxStatus::Blocked,
        ];

        for status in statuses {
            let c = status.to_char();
            let parsed = CheckboxStatus::from_char(c);
            assert_eq!(parsed, Some(status), "Failed to roundtrip {:?}", status);
        }
    }
}
