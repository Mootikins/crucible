#![allow(deprecated)]
mod auth_e2e_helpers;

use assert_cmd::Command;
use auth_e2e_helpers::AuthTestEnv;
use serial_test::serial;

#[test]
#[serial]
fn auth_login_stores_credential_non_interactive() {
    let env = AuthTestEnv::new();

    let mut cmd = env.command("auth");
    cmd.arg("login")
        .arg("--provider")
        .arg("openai")
        .arg("--key")
        .arg("sk-test-key-12345");

    cmd.assert().success();

    assert!(env.secrets_file_exists());
    assert_eq!(
        env.read_provider_key("openai"),
        Some("sk-test-key-12345".to_string())
    );
}

#[test]
#[serial]
fn auth_login_overwrites_existing_credential() {
    let env = AuthTestEnv::new().with_credential("openai", "sk-old-key");

    let mut cmd = env.command("auth");
    cmd.arg("login")
        .arg("--provider")
        .arg("openai")
        .arg("--key")
        .arg("sk-new-key");

    cmd.assert().success();

    assert_eq!(
        env.read_provider_key("openai"),
        Some("sk-new-key".to_string())
    );
}

#[test]
#[serial]
fn auth_login_multiple_providers() {
    let env = AuthTestEnv::new();

    env.command("auth")
        .arg("login")
        .arg("--provider")
        .arg("openai")
        .arg("--key")
        .arg("sk-openai")
        .assert()
        .success();

    env.command("auth")
        .arg("login")
        .arg("--provider")
        .arg("anthropic")
        .arg("--key")
        .arg("sk-ant-key")
        .assert()
        .success();

    assert_eq!(
        env.read_provider_key("openai"),
        Some("sk-openai".to_string())
    );
    assert_eq!(
        env.read_provider_key("anthropic"),
        Some("sk-ant-key".to_string())
    );
}

#[test]
#[serial]
fn auth_login_rejects_empty_key() {
    let env = AuthTestEnv::new();

    env.command("auth")
        .arg("login")
        .arg("--provider")
        .arg("openai")
        .arg("--key")
        .arg("")
        .assert()
        .failure();
}

#[test]
#[serial]
fn auth_logout_removes_credential() {
    let env = AuthTestEnv::new().with_credential("openai", "sk-test-key");
    assert!(env.read_provider_key("openai").is_some());

    env.command("auth")
        .arg("logout")
        .arg("--provider")
        .arg("openai")
        .assert()
        .success();

    assert!(env.read_provider_key("openai").is_none());
}

#[test]
#[serial]
fn auth_logout_nonexistent_provider_succeeds() {
    let env = AuthTestEnv::new();

    env.command("auth")
        .arg("logout")
        .arg("--provider")
        .arg("nonexistent")
        .assert()
        .success();
}

#[test]
#[serial]
fn auth_logout_preserves_other_providers() {
    let env = AuthTestEnv::new()
        .with_credential("openai", "sk-openai")
        .with_credential("anthropic", "sk-ant");

    env.command("auth")
        .arg("logout")
        .arg("--provider")
        .arg("openai")
        .assert()
        .success();

    assert!(env.read_provider_key("openai").is_none());
    assert_eq!(
        env.read_provider_key("anthropic"),
        Some("sk-ant".to_string())
    );
}

#[test]
#[serial]
fn auth_list_shows_stored_credentials() {
    let env = AuthTestEnv::new().with_credential("openai", "sk-test-key-12345");

    env.command("auth")
        .arg("list")
        .assert()
        .success()
        .stdout(predicates::str::contains("openai"))
        .stdout(predicates::str::contains("sk-te"));
}

#[test]
#[serial]
fn auth_list_shows_env_var_credentials() {
    let env = AuthTestEnv::new().with_env_var("OPENAI_API_KEY", "sk-from-env-12345");

    env.command("auth")
        .arg("list")
        .assert()
        .success()
        .stdout(predicates::str::contains("openai"))
        .stdout(predicates::str::contains("env"));
}

#[test]
#[serial]
fn auth_list_empty_shows_guidance() {
    let env = AuthTestEnv::new();

    env.command("auth")
        .arg("list")
        .assert()
        .success()
        .stdout(predicates::str::contains("cru auth login"));
}

#[test]
#[serial]
fn auth_roundtrip_login_list_logout() {
    let env = AuthTestEnv::new();

    env.command("auth")
        .arg("login")
        .arg("--provider")
        .arg("openai")
        .arg("--key")
        .arg("sk-roundtrip-key")
        .assert()
        .success();

    env.command("auth")
        .arg("list")
        .assert()
        .success()
        .stdout(predicates::str::contains("openai"));

    env.command("auth")
        .arg("logout")
        .arg("--provider")
        .arg("openai")
        .assert()
        .success();

    env.command("auth")
        .arg("list")
        .assert()
        .success()
        .stdout(predicates::str::contains("cru auth login"));
}

#[cfg(unix)]
#[test]
#[serial]
fn auth_login_creates_file_with_restricted_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let env = AuthTestEnv::new();

    env.command("auth")
        .arg("login")
        .arg("--provider")
        .arg("openai")
        .arg("--key")
        .arg("sk-perms-test")
        .assert()
        .success();

    let metadata = std::fs::metadata(env.secrets_file_path()).unwrap();
    let mode = metadata.permissions().mode() & 0o777;
    assert_eq!(mode, 0o600, "secrets.toml should have 0600 permissions");
}

#[test]
#[serial]
fn error_invalid_subcommand_shows_suggestion() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chta");

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("unrecognized subcommand"))
        .stderr(predicates::str::contains("chat"));
}

#[test]
#[serial]
fn error_missing_required_arg_shows_help() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("session").arg("resume");

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("required arguments"))
        .stderr(predicates::str::contains("Usage:"));
}

#[test]
#[serial]
fn error_conflicting_args_shows_message() {
    let mut cmd = Command::cargo_bin("cru").unwrap();
    cmd.arg("chat")
        .arg("--record")
        .arg("recording.jsonl")
        .arg("--replay")
        .arg("replay.jsonl");

    cmd.assert()
        .failure()
        .stderr(predicates::str::contains("cannot be used with"))
        .stderr(predicates::str::contains("--record"))
        .stderr(predicates::str::contains("--replay"));
}
