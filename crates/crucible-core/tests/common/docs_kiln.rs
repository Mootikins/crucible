//! Locating and walking the `docs/` kiln.
//!
//! Both `tests/dev_kiln.rs` and `tests/docs_config.rs` sweep the same tree with
//! the same anchoring. They used to carry a copy of this each, and the copies
//! drifted — only one of them skipped `.crucible/`, so whether a developer who
//! had chatted inside `docs/` failed the suite depended on which binary ran.

use std::path::{Path, PathBuf};
use walkdir::WalkDir;

/// The repository root: `crucible-core`'s manifest dir, up out of `crates/`.
///
/// Anchoring on `CARGO_MANIFEST_DIR` (not the process cwd) is what lets these
/// tests be run from anywhere, and is also why they can only ever validate this
/// repo's `docs/` — there is no way to point them at a copy.
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

/// The documentation kiln, `<workspace>/docs`.
pub fn docs_root() -> PathBuf {
    workspace_root().join("docs")
}

/// Every authored `.md` file under the given workspace-relative roots, sorted.
///
/// `.crucible/` is excluded: it holds session notes the daemon writes when
/// someone chats in this kiln. They are generated, not authored, so holding
/// them to the authoring conventions failed the suite for any developer who
/// had used `docs/` as a kiln.
pub fn markdown_files(roots: &[&str]) -> Vec<PathBuf> {
    files_with_extensions(roots, &["md"])
}

/// Every authored file with one of `extensions` under the given
/// workspace-relative roots, sorted.
pub fn files_with_extensions(roots: &[&str], extensions: &[&str]) -> Vec<PathBuf> {
    let workspace = workspace_root();
    let mut files: Vec<PathBuf> = roots
        .iter()
        .flat_map(|dir| WalkDir::new(workspace.join(dir)).into_iter().flatten())
        .filter(|e| e.file_type().is_file())
        .filter(|e| {
            e.path()
                .extension()
                .is_some_and(|ext| extensions.iter().any(|want| ext == *want))
        })
        .filter(|e| is_authored(e.path()))
        .map(|e| e.path().to_path_buf())
        .collect();
    files.sort();
    files
}

/// False for anything under a `.crucible/` directory — see [`markdown_files`].
pub fn is_authored(path: &Path) -> bool {
    !path.components().any(|c| c.as_os_str() == ".crucible")
}
