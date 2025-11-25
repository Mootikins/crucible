// New CLI commands (ACP-based)
pub mod chat;
pub mod mcp;
pub mod process;

// Existing commands (kept for compatibility)
pub mod config;
pub mod status;

// Old commands (to be removed post-MVP)
pub mod diff;
pub mod fuzzy; // Stub module that forwards to unified search
pub mod parse;
pub mod search;
pub mod secure_filesystem;
pub mod stats;
pub mod storage;

// Disabled commands
// pub mod semantic;  // Temporarily disabled - needs refactor to use new architecture

// Tests
#[cfg(test)]
mod tests;
