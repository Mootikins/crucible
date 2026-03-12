use crucible_config::{read_kiln_config, read_project_config};
use std::fs;
use tempfile::TempDir;

#[test]
fn read_kiln_config_falls_back_to_legacy_workspace_toml() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let crucible_dir = temp.path().join(".crucible");
    fs::create_dir_all(&crucible_dir).expect("failed to create .crucible dir");

    fs::write(
        crucible_dir.join("workspace.toml"),
        "[workspace]\nname = \"test-kiln\"\n",
    )
    .expect("failed to write workspace.toml");

    let config = read_kiln_config(temp.path()).expect("expected kiln config from fallback");
    assert_eq!(config.kiln.name, "test-kiln");
}

#[test]
fn read_project_config_falls_back_to_legacy_workspace_toml() {
    let temp = TempDir::new().expect("failed to create temp dir");
    let crucible_dir = temp.path().join(".crucible");
    fs::create_dir_all(&crucible_dir).expect("failed to create .crucible dir");

    fs::write(
        crucible_dir.join("workspace.toml"),
        "[workspace]\nname = \"test-project\"\n\n[[kilns]]\npath = \"docs\"\n\n[security.shell]\nwhitelist = [\"git\"]\n",
    )
    .expect("failed to write workspace.toml");

    let config = read_project_config(temp.path()).expect("expected project config from fallback");
    assert_eq!(config.kilns.len(), 1);
    assert_eq!(config.kilns[0].path.to_string_lossy(), "docs");
    assert_eq!(config.security.shell.whitelist, vec!["git".to_string()]);
}
