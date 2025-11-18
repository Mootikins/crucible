/// Shared utility functions for SurrealDB operations
///
/// This module provides common utilities for safe database operations,
/// particularly focused on preventing SQL injection and ensuring data integrity.

/// Sanitize and validate a record ID for safe use in SurrealDB queries
///
/// This provides defense-in-depth protection against SQL injection and malformed IDs:
/// - Validates length (1-255 characters)
/// - Rejects control characters and null bytes
/// - Sanitizes filesystem and SQL injection characters
/// - Ensures only safe characters remain
///
/// # Arguments
///
/// * `id` - The identifier to sanitize
///
/// # Returns
///
/// `Ok(String)` with the sanitized identifier safe for use in SurrealDB queries,
/// or `Err(String)` if the ID is invalid (empty or too long)
///
/// # Examples
///
/// ```
/// use crucible_surrealdb::utils::sanitize_record_id;
///
/// let safe_id = sanitize_record_id("my-safe-id").unwrap();
/// let dangerous_id = sanitize_record_id("'; DROP TABLE users; --").unwrap();
/// assert_eq!(dangerous_id, "__DROP_TABLE_users____");
/// ```
pub fn sanitize_record_id(id: &str) -> Result<String, String> {
    // Validate length
    if id.is_empty() {
        return Err("Record ID cannot be empty".to_string());
    }
    if id.len() > 255 {
        return Err(format!(
            "Record ID must be between 1 and 255 characters, got {} characters",
            id.len()
        ));
    }

    // Check for control characters and null bytes (security risk)
    if id.chars().any(|c| c.is_control() || c == '\0') {
        return Err("Record ID contains invalid control characters or null bytes".to_string());
    }

    // Sanitize: Replace all potentially dangerous characters with underscores
    // This includes:
    // - Filesystem separators: / \ :
    // - SQL injection characters: ' ` ; --
    // - Wildcards and special chars: * ? " < > |
    // - Whitespace (replace with underscore for clarity)
    let sanitized = id
        .chars()
        .map(|c| match c {
            // Filesystem separators
            '/' | '\\' | ':' => '_',
            // SQL injection risks (including backticks!)
            '\'' | '`' | ';' => '_',
            '-' if id.contains("--") => '_',
            // Wildcards and special characters
            '*' | '?' | '"' | '<' | '>' | '|' => '_',
            // Whitespace
            c if c.is_whitespace() => '_',
            // Allow alphanumeric, underscore, period, and hyphen
            c if c.is_ascii_alphanumeric() || c == '_' || c == '.' || c == '-' => c,
            // Replace anything else with underscore
            _ => '_',
        })
        .collect();

    Ok(sanitized)
}

/// Format a sanitized record ID for use in SurrealDB table:id syntax
///
/// This combines sanitization with proper formatting for SurrealDB record references.
/// It uses the angle bracket syntax `⟨...⟩` which is safer than backticks for complex IDs.
///
/// # Arguments
///
/// * `table` - The table name
/// * `id` - The record identifier to sanitize and format
///
/// # Returns
///
/// `Ok(String)` with the formatted record reference (e.g., "users:⟨abc123⟩"),
/// or `Err(String)` if the ID is invalid
///
/// # Examples
///
/// ```
/// use crucible_surrealdb::utils::format_record_id;
///
/// let record_ref = format_record_id("users", "john-doe").unwrap();
/// assert_eq!(record_ref, "users:⟨john-doe⟩");
/// ```
pub fn format_record_id(table: &str, id: &str) -> Result<String, String> {
    let sanitized = sanitize_record_id(id)?;
    Ok(format!("{}:⟨{}⟩", table, sanitized))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_sanitize_record_id_valid() {
        assert_eq!(
            sanitize_record_id("abc123").unwrap(),
            "abc123"
        );
        assert_eq!(
            sanitize_record_id("my-safe-id").unwrap(),
            "my-safe-id"
        );
        assert_eq!(
            sanitize_record_id("file.txt").unwrap(),
            "file.txt"
        );
    }

    #[test]
    fn test_sanitize_record_id_sql_injection() {
        // Single quotes should be replaced
        assert_eq!(
            sanitize_record_id("'; DROP TABLE users; --").unwrap(),
            "__DROP_TABLE_users____"
        );

        // Backticks should be replaced (the bug we're fixing!)
        assert_eq!(
            sanitize_record_id("`; DELETE FROM data; --").unwrap(),
            "__DELETE_FROM_data____"
        );
    }

    #[test]
    fn test_sanitize_record_id_path_traversal() {
        assert_eq!(
            sanitize_record_id("../../../etc/passwd").unwrap(),
            ".._.._..._etc_passwd"
        );
        assert_eq!(
            sanitize_record_id("C:\\Windows\\System32").unwrap(),
            "C__Windows_System32"
        );
    }

    #[test]
    fn test_sanitize_record_id_empty() {
        assert!(sanitize_record_id("").is_err());
    }

    #[test]
    fn test_sanitize_record_id_too_long() {
        let long_id = "a".repeat(256);
        assert!(sanitize_record_id(&long_id).is_err());
    }

    #[test]
    fn test_sanitize_record_id_control_chars() {
        assert!(sanitize_record_id("test\0null").is_err());
        assert!(sanitize_record_id("test\nnewline").is_err());
    }

    #[test]
    fn test_format_record_id() {
        assert_eq!(
            format_record_id("users", "john-doe").unwrap(),
            "users:⟨john-doe⟩"
        );
        assert_eq!(
            format_record_id("content_blocks", "abc123").unwrap(),
            "content_blocks:⟨abc123⟩"
        );
    }
}
