// New CLI commands (ACP-based)
pub mod agents;
pub mod auth;
pub mod chat;
pub mod completions;
pub mod daemon;
pub mod doctor;
pub mod init;
pub mod mcp;
pub mod models;
pub mod plugin;
pub mod process;
pub mod search;
pub mod session;
pub mod set;
pub mod skills;
pub mod stdin;
pub mod tasks;
pub mod tools;
#[cfg(feature = "web")]
pub mod web;

// Existing commands (kept for compatibility)
pub mod config;
pub mod status;

pub mod secure_filesystem;
pub mod stats;
pub mod storage;

#[cfg(test)]
mod chat_factory_tests;
