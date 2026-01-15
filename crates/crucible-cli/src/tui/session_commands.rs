//! Session and shell command handlers
//!
//! Provides utilities for:
//! - Opening session in external editor
//! - Executing shell commands
//! - Managing TUI mode transitions for external processes

use anyhow::Result;
use crossterm::{
    cursor,
    event::{DisableMouseCapture, EnableMouseCapture},
    execute,
    terminal::{
        self, disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen,
    },
};
use tracing::debug;

/// Result of opening a session in an editor
pub enum EditorResult {
    /// User saved the file
    Saved { path: std::path::PathBuf },
    /// User aborted without saving
    Aborted,
    /// Editor failed to launch
    Failed,
}

/// Get the editor command from environment
///
/// Checks $VISUAL first, then $EDITOR, then defaults to "vi"
pub fn get_editor() -> String {
    std::env::var("VISUAL")
        .or_else(|_| std::env::var("EDITOR"))
        .unwrap_or_else(|_| "vi".to_string())
}

/// Open a markdown file in the user's editor
///
/// This will:
/// 1. Temporarily exit TUI mode
/// 2. Launch the editor with the file
/// 3. Re-enter TUI mode
/// 4. Return the result
///
/// # Arguments
/// * `file_path` - Path to the file to edit
/// * `mouse_mode_enabled` - Whether to re-enable mouse mode after
///
/// # Returns
/// Result indicating whether the user saved or aborted
pub fn open_file_in_editor(
    file_path: &std::path::Path,
    mouse_mode_enabled: bool,
) -> Result<EditorResult> {
    let editor = get_editor();

    debug!(editor = %editor, path = %file_path.display(), "Opening file in editor");

    // Temporarily exit alternate screen so user can see editor
    execute!(
        std::io::stdout(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        cursor::Show
    )?;
    terminal::disable_raw_mode()?;

    // Open editor with the file
    let status = std::process::Command::new(&editor).arg(file_path).status();

    // Re-enter TUI mode
    terminal::enable_raw_mode()?;
    if mouse_mode_enabled {
        execute!(
            std::io::stdout(),
            EnableMouseCapture,
            EnterAlternateScreen,
            cursor::Hide
        )?;
    } else {
        execute!(std::io::stdout(), EnterAlternateScreen, cursor::Hide)?;
    }

    match status {
        Ok(exit_status) if exit_status.success() => Ok(EditorResult::Saved {
            path: file_path.to_path_buf(),
        }),
        Ok(_) => Ok(EditorResult::Aborted),
        Err(e) => {
            tracing::error!(error = %e, "Failed to launch editor");
            Ok(EditorResult::Failed)
        }
    }
}

/// Create a temporary file with the given content
///
/// # Arguments
/// * `content` - Content to write to the file
/// * `prefix` - Prefix for the temp file name
///
/// # Returns
/// Path to the created temp file
pub fn create_temp_file(content: &str, prefix: &str) -> Result<std::path::PathBuf> {
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!(
        "{}-{}.md",
        prefix,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap_or_default()
            .as_secs()
    ));

    let mut file = std::fs::File::create(&temp_file)?;
    std::io::Write::write_all(&mut file, content.as_bytes())?;
    file.sync_all()?;

    Ok(temp_file)
}

/// Open the session in an editor
///
/// Serializes the conversation to markdown and opens it in the user's editor.
///
/// # Arguments
/// * `markdown` - The session content as markdown
/// * `mouse_mode_enabled` - Whether to re-enable mouse mode after
///
/// # Returns
/// Result containing the path to the temp file if successful
pub fn open_session_in_editor(
    markdown: &str,
    mouse_mode_enabled: bool,
) -> Result<std::path::PathBuf> {
    let temp_file = create_temp_file(markdown, "crucible-session")?;

    match open_file_in_editor(&temp_file, mouse_mode_enabled)? {
        EditorResult::Saved { .. } => {
            debug!("User saved session in editor");
            Ok(temp_file)
        }
        EditorResult::Aborted => {
            debug!("User aborted editor");
            // Clean up temp file
            let _ = std::fs::remove_file(&temp_file);
            Ok(temp_file)
        }
        EditorResult::Failed => {
            // Clean up temp file
            let _ = std::fs::remove_file(&temp_file);
            Ok(temp_file)
        }
    }
}

pub fn edit_in_editor(content: &str, mouse_mode_enabled: bool) -> Result<Option<String>> {
    let temp_file = create_temp_file(content, "crucible-input")?;

    match open_file_in_editor(&temp_file, mouse_mode_enabled)? {
        EditorResult::Saved { .. } => {
            let edited = std::fs::read_to_string(&temp_file)?;
            let _ = std::fs::remove_file(&temp_file);
            Ok(Some(edited.trim_end().to_string()))
        }
        EditorResult::Aborted | EditorResult::Failed => {
            let _ = std::fs::remove_file(&temp_file);
            Ok(None)
        }
    }
}

/// Drop to an interactive shell with a command displayed
///
/// Instead of running the command directly, this spawns the user's shell
/// and prints the command for them to run. This allows for:
/// - Editing the command before running
/// - Running sudo commands (password prompt works)
/// - Chaining additional commands
/// - Full interactive shell access
///
/// # Arguments
/// * `command` - The command to display (not executed automatically)
/// * `mouse_mode_enabled` - Whether to re-enable mouse mode after
pub fn drop_to_shell(command: &str, mouse_mode_enabled: bool) -> Result<()> {
    use std::process::Command;

    debug!(cmd = %command, "Dropping to shell");

    // Exit TUI
    execute!(
        std::io::stdout(),
        DisableMouseCapture,
        LeaveAlternateScreen,
        cursor::Show
    )?;
    terminal::disable_raw_mode()?;

    // Print command and spawn user's shell
    println!("$ {}", command);
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "/bin/sh".into());

    let status = Command::new(&shell).status();

    // Re-enter TUI mode
    terminal::enable_raw_mode()?;
    if mouse_mode_enabled {
        execute!(
            std::io::stdout(),
            EnableMouseCapture,
            EnterAlternateScreen,
            cursor::Hide
        )?;
    } else {
        execute!(std::io::stdout(), EnterAlternateScreen, cursor::Hide)?;
    }

    match status {
        Ok(_) => Ok(()),
        Err(e) => {
            tracing::error!(error = %e, "Failed to spawn shell");
            Err(e.into())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_editor() {
        // Test that we get some editor (may vary by system)
        let editor = get_editor();
        assert!(!editor.is_empty());
    }

    #[test]
    fn test_create_temp_file() {
        let content = "Hello, world!\nThis is a test.";
        let temp_file = create_temp_file(content, "test-crucible").unwrap();

        // Verify file exists
        assert!(temp_file.exists());

        // Verify content
        let read_content = std::fs::read_to_string(&temp_file).unwrap();
        assert_eq!(read_content, content);

        // Clean up
        std::fs::remove_file(&temp_file).unwrap();
    }

    #[test]
    fn test_create_temp_file_different_prefixes() {
        let content1 = "content 1";
        let content2 = "content 2";

        let file1 = create_temp_file(content1, "prefix1").unwrap();
        let file2 = create_temp_file(content2, "prefix2").unwrap();

        // Files should be different
        assert_ne!(file1, file2);

        // Verify contents
        assert_eq!(std::fs::read_to_string(&file1).unwrap(), content1);
        assert_eq!(std::fs::read_to_string(&file2).unwrap(), content2);

        // Clean up
        std::fs::remove_file(&file1).unwrap();
        std::fs::remove_file(&file2).unwrap();
    }
}
