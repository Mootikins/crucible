use super::super::reference::{RefKind, parse_ref_kind};
use std::path::PathBuf;

#[test]
fn test_parse_ref_kind_file() {
    assert_eq!(
        parse_ref_kind("{file:test.toml}"),
        Some(RefKind::File(PathBuf::from("test.toml")))
    );
    assert_eq!(
        parse_ref_kind("{file:~/secrets/key.txt}"),
        Some(RefKind::File(PathBuf::from("~/secrets/key.txt")))
    );
    assert_eq!(
        parse_ref_kind("{file:/etc/crucible/config.toml}"),
        Some(RefKind::File(PathBuf::from("/etc/crucible/config.toml")))
    );

    assert_eq!(parse_ref_kind("test.toml"), None);
    assert_eq!(parse_ref_kind("{file:missing-end"), None);
    assert_eq!(parse_ref_kind("file:test.toml}"), None);
    assert_eq!(parse_ref_kind(""), None);
}

#[test]
fn test_parse_ref_kind_env() {
    assert_eq!(
        parse_ref_kind("{env:OPENAI_API_KEY}"),
        Some(RefKind::Env("OPENAI_API_KEY".to_string()))
    );
    assert_eq!(
        parse_ref_kind("{env:MY_VAR}"),
        Some(RefKind::Env("MY_VAR".to_string()))
    );
    assert_eq!(
        parse_ref_kind("{env:A}"),
        Some(RefKind::Env("A".to_string()))
    );

    assert_eq!(parse_ref_kind("OPENAI_API_KEY"), None);
    assert_eq!(parse_ref_kind("{env:missing-end"), None);
    assert_eq!(parse_ref_kind("env:VAR}"), None);
    assert_eq!(parse_ref_kind(""), None);
}

#[test]
fn test_parse_ref_kind_dir() {
    assert_eq!(
        parse_ref_kind("{dir:~/.config/crucible/providers.d/}"),
        Some(RefKind::Dir(PathBuf::from(
            "~/.config/crucible/providers.d/"
        )))
    );
    assert_eq!(
        parse_ref_kind("{dir:providers.d}"),
        Some(RefKind::Dir(PathBuf::from("providers.d")))
    );
    assert_eq!(
        parse_ref_kind("{dir:/etc/crucible/conf.d}"),
        Some(RefKind::Dir(PathBuf::from("/etc/crucible/conf.d")))
    );

    assert_eq!(parse_ref_kind("providers.d"), None);
    assert_eq!(parse_ref_kind("{dir:missing-end"), None);
    assert_eq!(parse_ref_kind("dir:path}"), None);
    assert_eq!(parse_ref_kind(""), None);
}
