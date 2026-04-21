use super::super::*;

// Path normalization tests
#[test]
fn normalize_path_traversal_attack() {
    assert_eq!(normalize_path_for_matching("src/../.env"), ".env");
}

#[test]
fn normalize_dot_components() {
    assert_eq!(normalize_path_for_matching("src/./main.rs"), "src/main.rs");
}

#[test]
fn normalize_double_slashes() {
    assert_eq!(
        normalize_path_for_matching("src//deep///file.rs"),
        "src/deep/file.rs"
    );
}

#[test]
fn normalize_trailing_slash() {
    assert_eq!(normalize_path_for_matching("src/deep/"), "src/deep");
}

#[test]
fn normalize_tilde_preserved() {
    assert_eq!(normalize_path_for_matching("~/Documents/"), "~/Documents");
}

#[test]
fn normalize_multiple_traversals() {
    assert_eq!(normalize_path_for_matching("a/b/../../c"), "c");
}

#[test]
fn normalize_traversal_above_root() {
    assert_eq!(
        normalize_path_for_matching("../../etc/passwd"),
        "../../etc/passwd"
    );
}

#[test]
fn normalize_empty_path() {
    assert_eq!(normalize_path_for_matching(""), "");
}

#[test]
fn normalize_single_dot() {
    assert_eq!(normalize_path_for_matching("."), "");
}

#[test]
fn normalize_single_dotdot() {
    assert_eq!(normalize_path_for_matching(".."), "..");
}

#[test]
fn normalize_absolute_path() {
    assert_eq!(normalize_path_for_matching("/etc/passwd"), "/etc/passwd");
}

#[test]
fn normalize_complex_traversal() {
    assert_eq!(normalize_path_for_matching("a/b/c/../../d/../e"), "a/e");
}

// split_chained_commands tests
#[test]
fn split_basic_and_operator() {
    let result = split_chained_commands("cargo test && rm -rf /");
    assert_eq!(result, vec!["cargo test", "rm -rf /"]);
}

#[test]
fn split_single_command_no_split() {
    let result = split_chained_commands("cargo test");
    assert_eq!(result, vec!["cargo test"]);
}

#[test]
fn split_double_quoted_string_no_split() {
    let result = split_chained_commands("echo \"hello && world\"");
    assert_eq!(result, vec!["echo \"hello && world\""]);
}

#[test]
fn split_single_quoted_string_no_split() {
    let result = split_chained_commands("git commit -m 'feat: && stuff'");
    assert_eq!(result, vec!["git commit -m 'feat: && stuff'"]);
}

#[test]
fn split_multiple_operators() {
    let result = split_chained_commands("a && b || c; d | e");
    assert_eq!(result, vec!["a", "b", "c", "d", "e"]);
}

#[test]
fn split_trailing_semicolon_filtered() {
    let result = split_chained_commands("cmd;");
    assert_eq!(result, vec!["cmd"]);
}

#[test]
fn split_empty_input() {
    let result = split_chained_commands("");
    assert_eq!(result, vec![] as Vec<&str>);
}

#[test]
fn split_whitespace_trimmed() {
    let result = split_chained_commands("  git commit -m 'feat: && stuff'  ");
    assert_eq!(result, vec!["git commit -m 'feat: && stuff'"]);
}

#[test]
fn split_pipe_operator() {
    let result = split_chained_commands("cat file.txt | grep pattern");
    assert_eq!(result, vec!["cat file.txt", "grep pattern"]);
}

#[test]
fn split_or_operator() {
    let result = split_chained_commands("cmd1 || cmd2");
    assert_eq!(result, vec!["cmd1", "cmd2"]);
}

#[test]
fn split_semicolon_operator() {
    let result = split_chained_commands("cmd1; cmd2");
    assert_eq!(result, vec!["cmd1", "cmd2"]);
}

#[test]
fn split_mixed_quotes() {
    let result = split_chained_commands("echo 'single' && echo \"double\"");
    assert_eq!(result, vec!["echo 'single'", "echo \"double\""]);
}

#[test]
fn split_nested_quotes_in_args() {
    let result = split_chained_commands("echo \"it's working\" && echo 'done'");
    assert_eq!(result, vec!["echo \"it's working\"", "echo 'done'"]);
}

#[test]
fn split_multiple_spaces_between_operators() {
    let result = split_chained_commands("cmd1  &&  cmd2");
    assert_eq!(result, vec!["cmd1", "cmd2"]);
}

#[test]
fn split_complex_command_with_args() {
    let result = split_chained_commands("cargo test --release && cargo build");
    assert_eq!(result, vec!["cargo test --release", "cargo build"]);
}
