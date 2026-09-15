use std::path::PathBuf;

mod connection;
mod creation;
mod io;
mod process_streaming;
mod protocol;
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
