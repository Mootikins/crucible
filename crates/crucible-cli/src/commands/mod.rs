// New CLI commands (ACP-based)
pub mod agents;
pub mod chat;
pub mod cluster;
pub mod mcp;
pub mod process;

// Existing commands (kept for compatibility)
pub mod config;
pub mod status;

pub mod secure_filesystem;
pub mod stats;
pub mod storage;

// Tests
#[cfg(test)]
mod tests;
