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
    /// The statement hands its command to another interpreter as data — `eval "$X"`,
    /// `sh -c "..."`. What runs is decided at runtime, so no inspection can name it.
    IndirectExecution,
    /// The command word is built by expansion — `r$Y -rf`, `${CMD} -rf`. Its name only
    /// exists once the shell has expanded it.
    ExpandedCommandWord,
}

impl UnmodellableConstruct {
    /// Short human-readable name, used in permission decision reasons.
    pub fn describe(self) -> &'static str {
        match self {
            Self::BacktickSubstitution => "backtick command substitution",
            Self::CommandSubstitution => "`$(...)` command substitution",
            Self::ProcessSubstitution => "process substitution",
            Self::UnterminatedQuote => "an unterminated quote",
            Self::IndirectExecution => "a command run through another interpreter",
            Self::ExpandedCommandWord => "a command name built by expansion",
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

// ── Command-word resolution ────────────────────────────────────────────────

/// Wrapper commands that run another command given as their argument. Stripping one
/// exposes the command it wraps, which is the thing a rule is written about:
/// `sudo rm -rf /` is an `rm` invocation however the rule's author spelled it.
///
/// `(name, flags that consume the following token, mandatory positionals)`. The flag table
/// keeps `sudo -u root rm` from resolving to `root`; the positional count keeps
/// `timeout 5 rm` from resolving to `5`.
/// What a wrapper's mandatory positional argument looks like. A wrapper only consumes one
/// when the token has the right shape — `timeout rm -rf /x` must resolve to `rm`, not eat
/// `rm` as a duration and hand back `-rf`. Guessing wrong here loses the command entirely,
/// so the shapes are deliberately narrow and an unrecognised token is treated as the
/// command rather than as the wrapper's argument.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Positional {
    /// Nothing mandatory before the command.
    None,
    /// `timeout 5`, `timeout 1.5m` — digits with an optional unit suffix.
    Duration,
    /// `flock /tmp/lock`, `chroot /jail` — a path or a file descriptor number.
    Path,
    /// `taskset 0x3` — a CPU mask.
    Mask,
}

impl Positional {
    /// Whether `token` is this wrapper's argument rather than the command it runs.
    fn matches(self, token: &str) -> bool {
        match self {
            Self::None => false,
            Self::Duration => {
                let body = token.trim_end_matches(['s', 'm', 'h', 'd']);
                !body.is_empty() && body.chars().all(|c| c.is_ascii_digit() || c == '.')
            }
            Self::Path => token.contains('/') || token.chars().all(|c| c.is_ascii_digit()),
            Self::Mask => {
                token.starts_with("0x") || token.chars().all(|c| c.is_ascii_digit() || c == ',')
            }
        }
    }
}

const WRAPPERS: &[(&str, &[&str], Positional)] = &[
    ("command", &[], Positional::None),
    ("builtin", &[], Positional::None),
    ("exec", &[], Positional::None),
    ("env", &["-u", "-C", "--unset", "--chdir"], Positional::None),
    ("nohup", &[], Positional::None),
    ("setsid", &[], Positional::None),
    ("time", &["-o", "-f"], Positional::None),
    (
        "timeout",
        &["-s", "-k", "--signal", "--kill-after"],
        Positional::Duration,
    ),
    ("nice", &["-n", "--adjustment"], Positional::None),
    (
        "ionice",
        &["-c", "-n", "-p", "--class", "--classdata", "--pid"],
        Positional::None,
    ),
    (
        "stdbuf",
        &["-i", "-o", "-e", "--input", "--output", "--error"],
        Positional::None,
    ),
    (
        "sudo",
        &[
            "-u", "-g", "-p", "-C", "-h", "--user", "--group", "--prompt",
        ],
        Positional::None,
    ),
    ("doas", &["-u", "-C"], Positional::None),
    ("watch", &["-n", "-d", "--interval"], Positional::None),
    ("parallel", &["-j", "-n", "--jobs"], Positional::None),
    ("strace", &["-o", "-p", "-e", "-s"], Positional::None),
    ("ltrace", &["-o", "-p", "-e"], Positional::None),
    ("unshare", &["--map-user", "--map-group"], Positional::None),
    (
        "runuser",
        &["-u", "-g", "--user", "--group"],
        Positional::None,
    ),
    (
        "systemd-run",
        &["-u", "-p", "--unit", "--property"],
        Positional::None,
    ),
    ("flock", &["-w", "-E", "--timeout"], Positional::Path),
    ("chroot", &["--userspec", "--groups"], Positional::Path),
    ("taskset", &["-p", "--pid"], Positional::Mask),
    (
        "xargs",
        &["-a", "-E", "-I", "-L", "-n", "-P", "-s", "-d", "--replace"],
        Positional::None,
    ),
];

/// Interpreters that take their program as *data*. No amount of prefix-stripping reveals
/// what `sh -c "$X"` will run, so these are reported rather than resolved.
const INDIRECT: &[&str] = &[
    // Shells.
    "eval",
    "sh",
    "bash",
    "zsh",
    "dash",
    "ksh",
    "fish",
    "busybox",
    "nix-shell",
    "su",
    // Other interpreters that take a program on the command line. `perl -e` is not a
    // different-program-same-effect case; it is this case with another binary name.
    "python",
    "python2",
    "python3",
    "perl",
    "ruby",
    "node",
    "deno",
    "bun",
    "php",
    "awk",
];

/// A statement rewritten so a rule written about a command matches the ways a shell can
/// spell that same command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedStatement {
    /// The statement with wrappers stripped, the command word reduced to its basename,
    /// and whitespace collapsed. Equal to the trimmed input when nothing applied.
    pub resolved: String,
    /// Set when the statement hands its command to another interpreter.
    pub unmodellable: Option<UnmodellableConstruct>,
}

/// Remove the quoting a shell removes before it resolves a command word.
///
/// A shell performs quote removal as its last expansion step, so `\rm`, `"rm"`, `'rm'`
/// and `r""m` all name the same command. Without this the wrapper table, the [`INDIRECT`]
/// table and the basename step all compare against the still-quoted token and miss.
/// `\rm` matters most: it is the standard way to bypass an alias.
///
/// Best-effort, and deliberately more aggressive than a shell inside double quotes, where
/// `\` escapes only a few characters. Over-removal can only produce a name that matches
/// no rule, and the resolved text is added to the restrictive lists rather than replacing
/// the raw text, so a wrong answer here can never unmatch a `deny`.
fn remove_quotes(token: &str) -> String {
    let mut out = String::with_capacity(token.len());
    let mut chars = token.chars();
    let (mut in_single, mut in_double) = (false, false);
    while let Some(c) = chars.next() {
        match c {
            '\'' if !in_double => in_single = !in_single,
            '"' if !in_single => in_double = !in_double,
            // Inside single quotes a backslash is literal; everywhere else it escapes.
            '\\' if !in_single => {
                if let Some(next) = chars.next() {
                    out.push(next);
                }
            }
            _ => out.push(c),
        }
    }
    out
}

/// Whether `token` is a `VAR=value` assignment prefix rather than a command word.
fn is_assignment_prefix(token: &str) -> bool {
    let Some((name, _)) = token.split_once('=') else {
        return false;
    };
    !name.is_empty()
        && !name.starts_with(|c: char| c.is_ascii_digit())
        && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
}

/// The command a statement actually invokes, as far as text inspection can tell.
///
/// **Best-effort by construction, and the residue is not small.** This closes the gap
/// between a rule naming a command and the ways a shell can spell *that same command*.
/// It does not, and cannot, decide what a statement will do.
///
/// Handled: grouping (`(`, `{`), `!`, `VAR=value` prefixes, the [`WRAPPERS`] table, a
/// leading directory on the command word, and tabs as separators. Reported rather than
/// guessed at: [`INDIRECT`] interpreters and a command word built by expansion.
///
/// Not handled, in rough order of how likely you are to meet it:
///
/// - **A different program with the same effect.** `find . -delete` and
///   `perl -e 'unlink ...'` delete files and are not `rm`. A rule naming `rm` does not
///   cover them and no resolution step can make it — deny the tool, or name the program.
/// - **A wrapper outside [`WRAPPERS`].** The table is a list, so an unlisted wrapper
///   hides what it runs. Adding one is a one-line change; discovering you needed to is
///   the problem.
/// - **Aliases and shell functions.** `alias rm=...`, or a function named `git` that
///   calls `rm`. Neither is visible in the statement text.
/// - **`$PATH` order.** `rm` resolving to something other than the `rm` you meant.
///
/// So this raises the cost of evading a `deny` rule; it does not make `deny` a sandbox.
/// Containment is the container, not this function.
pub fn resolve_command_word(statement: &str) -> ResolvedStatement {
    // Tabs and newlines are separators to a shell and glob-visible characters to us;
    // `rm\t-rf` must read as `rm -rf` or a rule about `rm ` never fires.
    let flattened = statement.replace(['\t', '\n', '\r', '\u{b}', '\u{c}'], " ");
    let mut tokens: Vec<&str> = flattened.split_whitespace().collect();
    let mut unmodellable = None;

    loop {
        // Grouping and negation can be glued to the command (`(rm`, `!rm`) or stand alone.
        while let Some(first) = tokens.first().copied() {
            let stripped = first.trim_start_matches(['(', '{', '!', ' ']);
            if stripped == first {
                break;
            }
            if stripped.is_empty() {
                tokens.remove(0);
            } else {
                tokens[0] = stripped;
            }
        }

        let Some(&head) = tokens.first() else { break };

        if is_assignment_prefix(head) {
            tokens.remove(0);
            continue;
        }

        // Unquote before the lookups, so `\sh -c` is still recognised as an interpreter
        // and `\sudo` as a wrapper. A wrapper is matched on its basename, so
        // `/usr/bin/sudo` strips too.
        let unquoted = remove_quotes(head);
        let head_base = unquoted.rsplit('/').next().unwrap_or(&unquoted);

        if INDIRECT.contains(&head_base) {
            unmodellable = Some(UnmodellableConstruct::IndirectExecution);
            break;
        }

        let Some((_, value_flags, positional)) =
            WRAPPERS.iter().find(|(name, _, _)| *name == head_base)
        else {
            break;
        };
        tokens.remove(0);

        // Skip the wrapper's own options. A flag known to take a value consumes the next
        // token as well, so `sudo -u root rm` does not resolve to `root`.
        while let Some(&opt) = tokens.first() {
            if opt == "--" {
                // End-of-options: the marker itself is not the command, so drop it and
                // stop. Leaving it made `sudo -- rm -rf /x` resolve to `--`.
                tokens.remove(0);
                break;
            }
            if !opt.starts_with('-') || opt == "-" {
                break;
            }
            tokens.remove(0);
            if value_flags.contains(&opt) && !tokens.is_empty() {
                tokens.remove(0);
            }
        }

        // Consume the wrapper's own positional only when it looks like one. An
        // unrecognised token is the command — `timeout rm -rf /x` resolves to `rm`.
        if tokens.len() > 1 && positional.matches(tokens[0]) {
            tokens.remove(0);
        }
    }

    if tokens.is_empty() {
        return ResolvedStatement {
            resolved: statement.trim().to_string(),
            unmodellable,
        };
    }

    // Read the expansion marker on the RAW token. Quote removal keeps `$`, but reading the
    // raw text is the conservative order: it can only report more, never fewer.
    if tokens[0].contains('$') {
        unmodellable = unmodellable.or(Some(UnmodellableConstruct::ExpandedCommandWord));
    }

    // `/bin/rm`, `./rm`, `\rm` and `"rm"` are the same invocation to anyone writing a rule.
    let unquoted_head = remove_quotes(tokens[0]);
    let base = unquoted_head
        .rsplit('/')
        .next()
        .unwrap_or(&unquoted_head)
        .to_string();
    let rest = tokens[1..].join(" ");

    let resolved = match (base.is_empty(), rest.is_empty()) {
        (true, _) => tokens.join(" "),
        (false, true) => base,
        (false, false) => format!("{base} {rest}"),
    };

    ResolvedStatement {
        resolved,
        unmodellable,
    }
}
