/// Check if a tool invocation matches a hardcoded Layer 0 denial pattern.
///
/// Returns `Some(reason)` if the operation is denied, `None` if allowed to continue.
/// These are immutable denials that cannot be overridden by any user config.
///
/// # Arguments
/// * `tool` - The tool name (e.g., "bash", "edit", "read")
/// * `input` - The input string (e.g., command for bash, path for edit)
///
/// # Examples
/// ```
/// use crucible_config::components::permissions::is_hardcoded_denied;
///
/// assert_eq!(
///     is_hardcoded_denied("bash", "rm -rf /"),
///     Some("Destructive: removes root filesystem")
/// );
/// assert_eq!(is_hardcoded_denied("bash", "cargo test"), None);
/// assert_eq!(is_hardcoded_denied("edit", "src/main.rs"), None);
/// ```
pub fn is_hardcoded_denied(tool: &str, input: &str) -> Option<&'static str> {
    // Only bash tool gets hardcoded denials
    if tool != "bash" {
        return None;
    }

    let trimmed = input.trim();

    // Pattern 1: rm -rf /
    if trimmed == "rm -rf /" || trimmed.starts_with("rm -rf / ") {
        return Some("Destructive: removes root filesystem");
    }

    // Pattern 2: rm -rf ~
    if trimmed == "rm -rf ~" || trimmed.starts_with("rm -rf ~ ") {
        return Some("Destructive: removes home directory");
    }

    // Pattern 3: rm -rf $HOME
    if trimmed == "rm -rf $HOME" || trimmed.starts_with("rm -rf $HOME ") {
        return Some("Destructive: removes home directory");
    }

    // Pattern 4: rm -rf .
    if trimmed == "rm -rf ." || trimmed.starts_with("rm -rf . ") {
        return Some("Destructive: removes current directory");
    }

    // Pattern 5: rm -rf ..
    if trimmed == "rm -rf .." || trimmed.starts_with("rm -rf .. ") {
        return Some("Destructive: removes parent directory");
    }

    // Pattern 6: sudo rm -rf * (with any path)
    if trimmed.starts_with("sudo rm -rf") && trimmed.contains("*") {
        return Some("Destructive: root removal with wildcard");
    }

    // Pattern 7: mkfs prefix (filesystem formatting)
    if trimmed.starts_with("mkfs") {
        return Some("Destructive: formats filesystem");
    }

    // Pattern 8: dd writing to block devices
    if trimmed.contains("dd ") && trimmed.contains("of=/dev/") {
        return Some("Destructive: writes to block device");
    }

    None
}
