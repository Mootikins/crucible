// New CLI commands (ACP-based)
pub mod agents;
pub mod auth;
pub mod chat;
pub mod daemon;
pub mod init;
pub mod mcp;
pub mod models;
pub mod process;
pub mod session;
pub mod skills;
pub mod tasks;

// Existing commands (kept for compatibility)
pub mod config;
pub mod status;

pub mod secure_filesystem;
pub mod stats;
pub mod storage;

#[cfg(test)]
mod chat_factory_tests;
