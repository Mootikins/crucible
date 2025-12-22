// New CLI commands (ACP-based)
pub mod agents;
pub mod chat;
pub mod daemon;
pub mod mcp;
pub mod process;
pub mod tasks;

// Existing commands (kept for compatibility)
pub mod config;
pub mod status;

pub mod secure_filesystem;
pub mod stats;
pub mod storage;

// Tests
#[cfg(test)]
mod tests;
