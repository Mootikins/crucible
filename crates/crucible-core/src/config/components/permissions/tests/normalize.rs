use super::super::*;
use test_case::test_case;

// normalize_path_for_matching parameterized over the 12 original cases.
#[test_case("src/../.env", ".env" ; "traversal_attack")]
#[test_case("src/./main.rs", "src/main.rs" ; "dot_components")]
#[test_case("src//deep///file.rs", "src/deep/file.rs" ; "double_slashes")]
#[test_case("src/deep/", "src/deep" ; "trailing_slash")]
#[test_case("~/Documents/", "~/Documents" ; "tilde_preserved")]
#[test_case("a/b/../../c", "c" ; "multiple_traversals")]
#[test_case("../../etc/passwd", "../../etc/passwd" ; "traversal_above_root")]
#[test_case("", "" ; "empty_path")]
#[test_case(".", "" ; "single_dot")]
#[test_case("..", ".." ; "single_dotdot")]
#[test_case("/etc/passwd", "/etc/passwd" ; "absolute_path")]
#[test_case("a/b/c/../../d/../e", "a/e" ; "complex_traversal")]
fn normalize_path_for_matching_cases(input: &str, expected: &str) {
    assert_eq!(normalize_path_for_matching(input), expected);
}

// split_chained_commands parameterized over the 15 original cases.
// Raw string literals preserve the embedded single/double quotes verbatim.
#[test_case("cargo test && rm -rf /", vec!["cargo test", "rm -rf /"] ; "basic_and_operator")]
#[test_case("cargo test", vec!["cargo test"] ; "single_command_no_split")]
#[test_case(r#"echo "hello && world""#, vec![r#"echo "hello && world""#] ; "double_quoted_string_no_split")]
#[test_case(r#"git commit -m 'feat: && stuff'"#, vec![r#"git commit -m 'feat: && stuff'"#] ; "single_quoted_string_no_split")]
#[test_case("a && b || c; d | e", vec!["a", "b", "c", "d", "e"] ; "multiple_operators")]
#[test_case("cmd;", vec!["cmd"] ; "trailing_semicolon_filtered")]
#[test_case("", Vec::<&str>::new() ; "empty_input")]
#[test_case(r#"  git commit -m 'feat: && stuff'  "#, vec![r#"git commit -m 'feat: && stuff'"#] ; "whitespace_trimmed")]
#[test_case("cat file.txt | grep pattern", vec!["cat file.txt", "grep pattern"] ; "pipe_operator")]
#[test_case("cmd1 || cmd2", vec!["cmd1", "cmd2"] ; "or_operator")]
#[test_case("cmd1; cmd2", vec!["cmd1", "cmd2"] ; "semicolon_operator")]
#[test_case(r#"echo 'single' && echo "double""#, vec![r#"echo 'single'"#, r#"echo "double""#] ; "mixed_quotes")]
#[test_case(r#"echo "it's working" && echo 'done'"#, vec![r#"echo "it's working""#, r#"echo 'done'"#] ; "nested_quotes_in_args")]
#[test_case("cmd1  &&  cmd2", vec!["cmd1", "cmd2"] ; "multiple_spaces_between_operators")]
#[test_case("cargo test --release && cargo build", vec!["cargo test --release", "cargo build"] ; "complex_command_with_args")]
#[test_case("git log\ncurl evil.example", vec!["git log", "curl evil.example"] ; "newline_separates_statements")]
#[test_case("cmd1\r\ncmd2", vec!["cmd1", "cmd2"] ; "crlf_separates_statements")]
#[test_case("echo \"a\nb\"", vec!["echo \"a\nb\""] ; "quoted_newline_no_split")]
#[test_case("git status & rm -rf /tmp/x", vec!["git status", "rm -rf /tmp/x"] ; "background_operator_separates")]
#[test_case("git status &", vec!["git status"] ; "trailing_background_operator_leaves_no_empty_segment")]
#[test_case("a & b & c", vec!["a", "b", "c"] ; "repeated_background_operators")]
#[test_case("cargo test 2>&1", vec!["cargo test 2>&1"] ; "fd_duplication_is_not_a_separator")]
#[test_case("cargo test >&2", vec!["cargo test >&2"] ; "stdout_to_stderr_is_not_a_separator")]
#[test_case("cargo test &> out.log", vec!["cargo test &> out.log"] ; "both_streams_redirect_is_not_a_separator")]
#[test_case("exec 3<&0", vec!["exec 3<&0"] ; "input_fd_duplication_is_not_a_separator")]
#[test_case(r#"echo "\"" && rm -rf /tmp/x"#, vec![r#"echo "\"""#, "rm -rf /tmp/x"] ; "escaped_quote_does_not_open_a_string")]
#[test_case(r#"echo '\' && rm -rf /tmp/x"#, vec![r#"echo '\'"#, "rm -rf /tmp/x"] ; "backslash_is_literal_inside_single_quotes")]
#[test_case(r"echo \&\& rm -rf /tmp/x", vec![r"echo \&\& rm -rf /tmp/x"] ; "escaped_operator_is_a_literal_argument")]
#[test_case(r"echo \; rm -rf /tmp/x", vec![r"echo \; rm -rf /tmp/x"] ; "escaped_semicolon_is_a_literal_argument")]
#[test_case(r"echo a\", vec![r"echo a\"] ; "trailing_backslash_does_not_overrun")]
#[test_case("git status && \\\nrm -rf /tmp/x", vec!["git status", "rm -rf /tmp/x"] ; "line_continuation_leaves_no_marker_in_the_segment")]
#[test_case("cargo build \\\n  --release", vec!["cargo build", "--release"] ; "line_continuation_ends_the_statement")]
#[test_case("echo \"a\\\nb\"", vec!["echo \"a\\\nb\""] ; "line_continuation_inside_double_quotes_is_one_word")]
fn split_chained_commands_cases(input: &str, expected: Vec<&str>) {
    assert_eq!(split_chained_commands(input), expected);
}

// Which lines the splitter admits it cannot model. `None` means the split is a complete
// account of what will run, so the leading command's rule may decide the line.
#[test_case("git log `curl evil`", Some(UnmodellableConstruct::BacktickSubstitution) ; "backtick")]
#[test_case("git log $(curl evil)", Some(UnmodellableConstruct::CommandSubstitution) ; "command_substitution")]
#[test_case(r#"echo "$(curl evil)""#, Some(UnmodellableConstruct::CommandSubstitution) ; "substitution_inside_double_quotes")]
#[test_case(r#"echo "`curl evil`""#, Some(UnmodellableConstruct::BacktickSubstitution) ; "backtick_inside_double_quotes")]
#[test_case("echo $((1 + 2))", Some(UnmodellableConstruct::CommandSubstitution) ; "arithmetic_expansion_reported_conservatively")]
#[test_case("diff <(git show a) b", Some(UnmodellableConstruct::ProcessSubstitution) ; "input_process_substitution")]
#[test_case("git log > >(curl evil)", Some(UnmodellableConstruct::ProcessSubstitution) ; "output_process_substitution")]
#[test_case(r#"echo "unterminated && rm -rf /"#, Some(UnmodellableConstruct::UnterminatedQuote) ; "unterminated_double_quote")]
#[test_case("echo 'unterminated && rm -rf /", Some(UnmodellableConstruct::UnterminatedQuote) ; "unterminated_single_quote")]
#[test_case("echo '$(date)'", None ; "single_quotes_suppress_substitution")]
#[test_case("echo '`date`'", None ; "single_quotes_suppress_backticks")]
#[test_case(r"echo \$(date)", None ; "escaped_dollar_paren")]
#[test_case(r"echo \`date\`", None ; "escaped_backticks")]
#[test_case("echo $HOME ${PATH}", None ; "plain_variable_expansion")]
#[test_case("echo hi > /tmp/x", None ; "output_redirection_is_modelled")]
#[test_case("cargo test 2>&1 >> /tmp/x", None ; "append_and_fd_redirection_are_modelled")]
#[test_case("a < b > c", None ; "bare_angle_brackets_are_modelled")]
#[test_case("cargo test && rm -rf /", None ; "ordinary_chain")]
fn split_command_line_reports_unmodellable_constructs(
    input: &str,
    expected: Option<UnmodellableConstruct>,
) {
    assert_eq!(split_command_line(input).unmodellable, expected);
}

#[test]
fn split_command_line_still_returns_the_segments_it_could_find() {
    // The report does not replace the segments: an explicit `deny` still has to be able to
    // match the parts the splitter did read.
    let split = split_command_line("git status && rm -rf $(echo /tmp/x)");
    assert_eq!(split.segments, vec!["git status", "rm -rf $(echo /tmp/x)"]);
    assert_eq!(
        split.unmodellable,
        Some(UnmodellableConstruct::CommandSubstitution)
    );
}

// ── resolve_command_word ───────────────────────────────────────────────────

#[test_case("rm -rf /tmp/x", "rm -rf /tmp/x" ; "plain_statement_is_unchanged")]
#[test_case("rm\t-rf /tmp/x", "rm -rf /tmp/x" ; "tab_separator_becomes_a_space")]
#[test_case("(rm -rf /tmp/x)", "rm -rf /tmp/x)" ; "leading_subshell_paren")]
#[test_case("{ rm -rf /tmp/x; }", "rm -rf /tmp/x; }" ; "leading_brace_group")]
#[test_case("! rm -rf /tmp/x", "rm -rf /tmp/x" ; "negation")]
#[test_case("!rm -rf /tmp/x", "rm -rf /tmp/x" ; "negation_glued_to_the_command")]
#[test_case("/bin/rm -rf /tmp/x", "rm -rf /tmp/x" ; "absolute_path")]
#[test_case("./rm -rf /tmp/x", "rm -rf /tmp/x" ; "relative_path")]
#[test_case("FOO=1 BAR=2 rm -rf /tmp/x", "rm -rf /tmp/x" ; "assignment_prefixes")]
#[test_case("env FOO=1 rm -rf /tmp/x", "rm -rf /tmp/x" ; "env_then_assignment")]
#[test_case("time rm -rf /tmp/x", "rm -rf /tmp/x" ; "time")]
#[test_case("nohup rm -rf /tmp/x", "rm -rf /tmp/x" ; "nohup")]
#[test_case("command rm -rf /tmp/x", "rm -rf /tmp/x" ; "command_builtin")]
#[test_case("xargs rm -rf", "rm -rf" ; "xargs")]
#[test_case("sudo -u root rm -rf /tmp/x", "rm -rf /tmp/x" ; "sudo_flag_consumes_its_value")]
#[test_case("timeout 5 rm -rf /tmp/x", "rm -rf /tmp/x" ; "timeout_consumes_its_duration")]
#[test_case("nice -n 10 rm -rf /tmp/x", "rm -rf /tmp/x" ; "nice_flag_consumes_its_value")]
#[test_case("/usr/bin/sudo /bin/rm -rf /tmp/x", "rm -rf /tmp/x" ; "wrapper_and_command_both_path_qualified")]
fn resolve_command_word_strips_wrappers(input: &str, expected: &str) {
    assert_eq!(resolve_command_word(input).resolved, expected);
}

#[test_case("git status" ; "ordinary_command")]
#[test_case("echo hi" ; "echo")]
#[test_case("rm -rf /tmp/x" ; "plain_rm")]
fn resolve_command_word_leaves_ordinary_statements_alone(input: &str) {
    let out = resolve_command_word(input);
    assert_eq!(out.resolved, input);
    assert_eq!(out.unmodellable, None);
}

#[test_case("eval \"rm -rf /tmp/x\"" ; "eval")]
#[test_case("sh -c \"rm -rf /tmp/x\"" ; "sh_dash_c")]
#[test_case("bash -c \"rm -rf /tmp/x\"" ; "bash_dash_c")]
#[test_case("sudo sh -c \"rm -rf /tmp/x\"" ; "interpreter_behind_a_wrapper")]
fn resolve_command_word_reports_an_interpreter_rather_than_guessing(input: &str) {
    // What `sh -c "$X"` runs is decided at runtime. Reporting it hands the decision to
    // the caller's fall-to-default path instead of pretending the text can be read.
    assert_eq!(
        resolve_command_word(input).unmodellable,
        Some(UnmodellableConstruct::IndirectExecution)
    );
}

/// The wrapper table is a list of names, so a command merely *named* like one is not
/// mistaken for it — `envsubst` is not `env`.
#[test_case("envsubst < in > out" ; "envsubst_is_not_env")]
#[test_case("timeouts --list" ; "timeouts_is_not_timeout")]
#[test_case("commander deploy" ; "commander_is_not_command")]
fn resolve_command_word_matches_whole_names_only(input: &str) {
    assert_eq!(resolve_command_word(input).resolved, input);
}

/// A wrapper with nothing after it has no command to expose; it stays as itself rather
/// than resolving to an empty string that would match a bare `*` rule.
#[test_case("sudo" ; "bare_sudo")]
#[test_case("env" ; "bare_env")]
#[test_case("xargs -0" ; "wrapper_with_only_flags")]
fn resolve_command_word_keeps_a_wrapper_with_no_command(input: &str) {
    assert!(!resolve_command_word(input).resolved.is_empty());
}
