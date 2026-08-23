//! Tests for note CRUD tools.

mod crud;
mod indexed;
mod list;
mod path_safety;

use std::fs;
use std::sync::Arc;
use tempfile::TempDir;

use super::NoteTools;
use crate::empty_providers::EmptyKnowledgeRepository;

/// Tools over a kiln with no index at all, so every read goes to disk.
pub(super) fn unindexed(kiln_path: String) -> NoteTools {
    NoteTools::new(kiln_path, Arc::new(EmptyKnowledgeRepository))
}

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
