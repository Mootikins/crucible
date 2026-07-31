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

/// [`exe_relative`] for the currently running binary, or empty if the OS will
/// not say where it is.
pub fn for_current_exe() -> Vec<PathBuf> {
    std::env::current_exe()
        .ok()
        .and_then(|exe| exe.parent().map(exe_relative))
        .map(Vec::from)
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

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
