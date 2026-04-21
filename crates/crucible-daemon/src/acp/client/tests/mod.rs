use std::path::PathBuf;

use super::types::StreamingState;
use crucible_core::types::acp::ToolCallInfo;

mod connection;
mod creation;
mod diff;
mod io;
mod process_streaming;
mod protocol;
mod session;
mod streaming;

/// Cross-platform test path helper
pub(super) fn test_path(name: &str) -> PathBuf {
    std::env::temp_dir().join(format!("crucible_test_{}", name))
}

// Helper to get a simple command that runs and exits (like true/echo)
pub(super) fn get_simple_command() -> (PathBuf, Option<Vec<String>>) {
    #[cfg(windows)]
    {
        (
            PathBuf::from("cmd"),
            Some(vec!["/C".to_string(), "echo".to_string(), "ok".to_string()]),
        )
    }
    #[cfg(not(windows))]
    {
        (PathBuf::from("echo"), Some(vec!["ok".to_string()]))
    }
}

// Helper to get a command that echoes stdin to stdout (like cat)
pub(super) fn get_cat_command() -> (PathBuf, Option<Vec<String>>) {
    #[cfg(windows)]
    {
        // findstr can hang waiting for EOF. Use cmd hack to read one line and echo it.
        // This works for tests sending single messages.
        (
            PathBuf::from("cmd"),
            Some(vec![
                "/V".to_string(),
                "/C".to_string(),
                "set /p l= && echo !l!".to_string(),
            ]),
        )
    }
    #[cfg(not(windows))]
    {
        (PathBuf::from("cat"), None)
    }
}

// Helper to get a command that sleeps (for timeout tests)
pub(super) fn get_sleep_command() -> (PathBuf, Option<Vec<String>>) {
    #[cfg(windows)]
    {
        // Use ping hack for sleep to avoid heavy PowerShell startup
        // -n 6 pinging localhost approximates 5 seconds sleep
        (
            PathBuf::from("cmd"),
            Some(vec![
                "/C".to_string(),
                "ping 127.0.0.1 -n 6 > nul".to_string(),
            ]),
        )
    }
    #[cfg(not(windows))]
    {
        (PathBuf::from("sleep"), Some(vec!["5".to_string()]))
    }
}

/// Helper to test upsert logic in isolation
pub(super) fn upsert_tool_info(tool_call: ToolCallInfo, state: &mut StreamingState) {
    if let Some(id) = &tool_call.id {
        if let Some(existing) = state
            .tool_calls
            .iter_mut()
            .find(|t| t.id.as_deref() == Some(id.as_str()))
        {
            *existing = tool_call;
            return;
        }
    }
    state.tool_calls.push(tool_call);
}
