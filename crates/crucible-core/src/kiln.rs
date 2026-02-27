//! Kiln (workspace) constants and utilities

/// Directories to exclude from file discovery and watching
pub const EXCLUDED_DIRS: &[&str] = &[".crucible", ".git", ".obsidian", "node_modules", ".trash"];
