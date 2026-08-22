//! Tilde expansion for configured paths.
//!
//! One expander for every crate. Earlier copies disagreed on the bare `~`
//! case, and one copy panicked on it.

use std::path::{Path, PathBuf};

/// Expand a leading `~/` (or a bare `~`) in `raw` relative to `home`.
///
/// `home = None` leaves the `~` prefix unexpanded. A caller that checks
/// containment then rejects the path instead of resolving it to a
/// surprising place.
pub fn expand_tilde(raw: &str, home: Option<&Path>) -> PathBuf {
    match (raw.strip_prefix("~/"), raw, home) {
        (Some(rest), _, Some(home)) => home.join(rest),
        (None, "~", Some(home)) => home.to_path_buf(),
        _ => PathBuf::from(raw),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_tilde_prefix_joins_the_home_directory() {
        let home = Path::new("/home/u");
        assert_eq!(expand_tilde("~/notes", Some(home)), home.join("notes"));
    }

    #[test]
    fn a_bare_tilde_is_the_home_directory() {
        let home = Path::new("/home/u");
        assert_eq!(expand_tilde("~", Some(home)), home);
    }

    #[test]
    fn a_tilde_without_a_home_stays_unexpanded() {
        assert_eq!(expand_tilde("~/notes", None), PathBuf::from("~/notes"));
    }

    #[test]
    fn an_absolute_path_is_unchanged() {
        let home = Path::new("/home/u");
        assert_eq!(expand_tilde("/abs/p", Some(home)), PathBuf::from("/abs/p"));
    }

    #[test]
    fn a_tilde_inside_the_path_is_not_expanded() {
        let home = Path::new("/home/u");
        assert_eq!(expand_tilde("a/~/b", Some(home)), PathBuf::from("a/~/b"));
    }
}
