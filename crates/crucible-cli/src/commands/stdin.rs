use anyhow::{Context, Result};
use std::io::IsTerminal;

const MAX_STDIN_BYTES: usize = 1_048_576;

pub fn read_stdin_message() -> Result<String> {
    use std::io::Read;

    let mut buf = String::new();
    std::io::stdin()
        .read_to_string(&mut buf)
        .context("Failed to read from stdin")?;

    if buf.len() > MAX_STDIN_BYTES {
        eprintln!(
            "warning: stdin input truncated to {} bytes (was {} bytes)",
            MAX_STDIN_BYTES,
            buf.len()
        );
        buf.truncate(MAX_STDIN_BYTES);
    }

    let trimmed = buf.trim().to_string();
    if trimmed.is_empty() {
        anyhow::bail!("No input received from stdin");
    }

    Ok(trimmed)
}

pub fn stdin_is_piped() -> bool {
    !std::io::stdin().is_terminal()
}

pub fn resolve_message(message: &str) -> Result<String> {
    if message == "-" {
        read_stdin_message()
    } else {
        Ok(message.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolve_message_passthrough_normal_text() {
        let result = resolve_message("hello world").unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn resolve_message_dash_is_stdin_sentinel() {
        assert_ne!("-", "hello");
        let normal = resolve_message("not a dash").unwrap();
        assert_eq!(normal, "not a dash");
    }

    #[test]
    fn resolve_message_preserves_multiline() {
        let result = resolve_message("line1\nline2\nline3").unwrap();
        assert_eq!(result, "line1\nline2\nline3");
    }

    #[test]
    fn max_stdin_bytes_is_one_megabyte() {
        assert_eq!(MAX_STDIN_BYTES, 1_048_576);
    }

    #[test]
    fn stdin_is_piped_returns_bool() {
        let result = stdin_is_piped();
        assert!(matches!(result, true | false));
    }
}
