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
fn split_chained_commands_cases(input: &str, expected: Vec<&str>) {
    assert_eq!(split_chained_commands(input), expected);
}
