//! Where the shipped `runtime/` tree lives, relative to the running binary.
//!
//! Four subsystems look for it — plugins, `defaults/init.lua`, bundled skills,
//! and `cru setup`'s copy source — and each used to open-code the candidate
//! list. They drifted: skills discovery tried only the dev layout, so an
//! installed `cru` silently loaded no bundled help skills at all. One list,
//! one order, four callers.
//!
//! Lives in `crucible-core` because two of the four callers are in the CLI.

use std::path::{Path, PathBuf};

/// Runtime roots to try for a binary at `exe_dir`, highest priority first.
///
/// Installed before dev: a `~/.local/bin/cru` has a real
/// `~/.local/share/crucible/runtime`, whereas `<exe_dir>/../../runtime` for
/// that same binary is `~/runtime` — a path that is almost always absent and,
/// if it does exist, is not Crucible's.
///
/// Neither is checked for existence here; callers filter, because what counts
/// as present differs (a `plugins/` subdir, a `defaults/init.lua`, any
/// `*/skills`).
pub fn exe_relative(exe_dir: &Path) -> [PathBuf; 2] {
    [
        // Installed: <prefix>/bin/cru → <prefix>/share/crucible/runtime
        exe_dir
            .join("..")
            .join("share")
            .join("crucible")
            .join("runtime"),
        // Dev: <repo>/target/debug/cru → <repo>/runtime
        exe_dir.join("..").join("..").join("runtime"),
    ]
}

/// Where `cru setup` copies the runtime tree: `~/.config/crucible/runtime`.
///
/// A user-owned root, tried before anything shipped alongside the binary, so
/// `cru setup` is a way to *override* the bundled tree and not merely to
/// duplicate it.
pub fn user_runtime() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("crucible").join("runtime"))
}

/// Every root to try for the running binary, highest priority first.
///
/// `cru setup` copied the runtime into `~/.config/crucible/runtime` and then
/// printed instructions to set `CRUCIBLE_RUNTIME` by hand, because no resolver
/// looked there — a path written by one component and read by none, which is
/// the same defect this module exists to prevent. It is a root now, so `cru
/// setup` needs no follow-up.
pub fn for_current_exe() -> Vec<PathBuf> {
    let exe_relative = std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(exe_relative))
        .map(Vec::from)
        .unwrap_or_default();

    user_runtime().into_iter().chain(exe_relative).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `cru setup`'s target outranks anything shipped with the binary.
    ///
    /// It is the user saying "use this tree", so it has to be able to shadow
    /// the bundled one rather than merely sit behind it.
    #[test]
    fn the_user_runtime_is_tried_before_anything_shipped() {
        let Some(user) = user_runtime() else {
            return; // no config dir on this box; nothing to order
        };
        let roots = for_current_exe();
        assert_eq!(
            roots.first(),
            Some(&user),
            "cru setup's target is the highest-priority root"
        );
    }

    #[test]
    fn the_installed_layout_is_tried_before_the_dev_tree() {
        let roots = exe_relative(Path::new("/usr/local/bin"));
        assert_eq!(
            roots[0],
            Path::new("/usr/local/bin/../share/crucible/runtime")
        );
        assert_eq!(roots[1], Path::new("/usr/local/bin/../../runtime"));
    }
}
