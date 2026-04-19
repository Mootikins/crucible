//! Tests for note CRUD tools.

mod crud;
mod list;
mod note_store;
mod path_safety;

use std::fs;
use tempfile::TempDir;

pub(super) fn create_name_resolution_kiln() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::create_dir_all(dir.path().join("Meta")).unwrap();
    fs::write(
        dir.path().join("Meta/Plugin User Stories.md"),
        "# Plugin User Stories\n\nSubdirectory note",
    )
    .unwrap();
    fs::write(dir.path().join("README.md"), "# README\n\nRoot note").unwrap();
    dir
}
