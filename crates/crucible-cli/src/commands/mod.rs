// New CLI commands (ACP-based)
pub mod chat;
pub mod process;

// Existing commands (kept for compatibility)
pub mod config;
pub mod status;

// Old commands (to be removed post-MVP)
pub mod diff;
pub mod fuzzy;
pub mod fuzzy_interactive;
pub mod parse;
pub mod repl;
pub mod search;
pub mod secure_filesystem;
pub mod stats;
pub mod storage;

// Disabled commands
// pub mod semantic;  // Temporarily disabled - needs refactor to use new architecture
