/// Normalize a path for permission matching by collapsing `.`, `..`, and duplicate slashes.
///
/// This prevents path traversal attacks like `src/../.env` from bypassing file permission rules.
/// The function does NOT resolve symlinks or expand `~` (no filesystem access).
///
/// # Algorithm
///
/// 1. Split path on `/`
/// 2. Process each component:
///    - `.` → skip (current directory)
///    - `..` → pop from result stack if non-empty (parent directory)
///    - Other → push to result stack
/// 3. Rejoin with `/`
/// 4. Remove trailing slashes
///
/// # Examples
///
/// ```
/// # use crate::config::components::permissions::normalize_path_for_matching;
/// assert_eq!(normalize_path_for_matching("src/../.env"), ".env");
/// assert_eq!(normalize_path_for_matching("src/./main.rs"), "src/main.rs");
/// assert_eq!(normalize_path_for_matching("src//deep///file.rs"), "src/deep/file.rs");
/// assert_eq!(normalize_path_for_matching("src/deep/"), "src/deep");
/// assert_eq!(normalize_path_for_matching("~/Documents/"), "~/Documents");
/// assert_eq!(normalize_path_for_matching("a/b/../../c"), "c");
/// assert_eq!(normalize_path_for_matching("../../etc/passwd"), "../../etc/passwd");
/// assert_eq!(normalize_path_for_matching(""), "");
/// ```
///
/// # Security Note
///
/// For permission matching, apply normalization to BOTH the rule pattern and the input path,
/// then use the most restrictive result: if either the raw path OR the normalized path matches
/// a deny rule, deny the operation.
pub fn normalize_path_for_matching(path: &str) -> String {
    if path.is_empty() {
        return String::new();
    }

    let is_absolute = path.starts_with('/');
    let mut result: Vec<&str> = Vec::new();

    for component in path.split('/') {
        match component {
            "" | "." => {
                // Empty (from double slashes) or current dir → skip
            }
            ".." => {
                if let Some(last) = result.last() {
                    if *last != ".." {
                        result.pop();
                    } else if !is_absolute {
                        result.push("..");
                    }
                } else if !is_absolute {
                    result.push("..");
                }
            }
            other => {
                // Regular component → push
                result.push(other);
            }
        }
    }

    let normalized = result.join("/").trim_end_matches('/').to_string();
    if is_absolute {
        if normalized.is_empty() {
            "/".to_string()
        } else {
            format!("/{normalized}")
        }
    } else {
        normalized
    }
}

/// Split a bash command string on operators (`&&`, `||`, `;`, `|`) while respecting quoted strings.
///
/// This function splits chained bash commands so the permission engine can evaluate each
/// sub-command independently. It handles double-quoted and single-quoted strings, ensuring
/// operators inside quotes are not treated as delimiters.
///
/// # Arguments
/// * `input` - A bash command string, possibly containing multiple chained commands
///
/// # Returns
/// A vector of borrowed string slices, each representing a single command. Each slice is
/// trimmed of leading/trailing whitespace. Empty segments (e.g., from trailing semicolons)
/// are filtered out.
///
/// # Examples
/// ```
/// use crate::config::components::permissions::split_chained_commands;
///
/// // Basic chaining
/// assert_eq!(
///     split_chained_commands("cargo test && rm -rf /"),
///     vec!["cargo test", "rm -rf /"]
/// );
///
/// // Single command (no split)
/// assert_eq!(
///     split_chained_commands("cargo test"),
///     vec!["cargo test"]
/// );
///
/// // Quoted strings (no split inside quotes)
/// assert_eq!(
///     split_chained_commands("echo \"hello && world\""),
///     vec!["echo \"hello && world\""]
/// );
///
/// // Single-quoted strings
/// assert_eq!(
///     split_chained_commands("echo 'hello && world'"),
///     vec!["echo 'hello && world'"]
/// );
///
/// // Multiple operators
/// assert_eq!(
///     split_chained_commands("a && b || c; d | e"),
///     vec!["a", "b", "c", "d", "e"]
/// );
///
/// // Trailing semicolon (filtered out)
/// assert_eq!(
///     split_chained_commands("cmd;"),
///     vec!["cmd"]
/// );
///
/// // Empty input
/// assert_eq!(
///     split_chained_commands(""),
///     vec![] as Vec<&str>
/// );
/// ```
///
/// # Limitations
/// - Does not handle escaped operators like `\&\&` (treated as regular characters)
/// - Does not handle `$(...)` subshell substitution
/// - Does not handle heredocs
/// - Does not handle backtick substitution
///
/// # Security Note
/// This function is used for permission checking. Each sub-command is evaluated independently,
/// so `cargo test && rm -rf /` will check both `cargo test` and `rm -rf /` separately.
pub fn split_chained_commands(input: &str) -> Vec<&str> {
    if input.is_empty() {
        return Vec::new();
    }

    let bytes = input.as_bytes();
    let mut segments = Vec::new();
    let mut current_start = 0;
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    let mut i = 0;

    while i < bytes.len() {
        let ch = bytes[i];

        // Handle quote state
        if ch == b'"' && !in_single_quote {
            in_double_quote = !in_double_quote;
            i += 1;
            continue;
        }

        if ch == b'\x27' && !in_double_quote {
            in_single_quote = !in_single_quote;
            i += 1;
            continue;
        }

        // If inside quotes, skip operator detection
        if in_double_quote || in_single_quote {
            i += 1;
            continue;
        }

        // Check for operators (only when not in quotes)
        let is_operator = if i + 1 < bytes.len() {
            // Two-character operators: &&, ||
            (ch == b'&' && bytes[i + 1] == b'&') || (ch == b'|' && bytes[i + 1] == b'|')
        } else {
            false
        };

        if is_operator {
            // Found && or ||
            let segment = input[current_start..i].trim();
            if !segment.is_empty() {
                segments.push(segment);
            }
            current_start = i + 2;
            i += 2;
        } else if ch == b';' || ch == b'|' {
            // Single-character operators: ; or |
            let segment = input[current_start..i].trim();
            if !segment.is_empty() {
                segments.push(segment);
            }
            current_start = i + 1;
            i += 1;
        } else {
            i += 1;
        }
    }

    // Add the final segment
    let segment = input[current_start..].trim();
    if !segment.is_empty() {
        segments.push(segment);
    }

    segments
}
