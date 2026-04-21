use super::super::*;

#[test]
fn hardcoded_denied_rm_rf_root() {
    assert_eq!(
        is_hardcoded_denied("bash", "rm -rf /"),
        Some("Destructive: removes root filesystem")
    );
}

#[test]
fn hardcoded_denied_rm_rf_home_tilde() {
    assert_eq!(
        is_hardcoded_denied("bash", "rm -rf ~"),
        Some("Destructive: removes home directory")
    );
}

#[test]
fn hardcoded_denied_rm_rf_home_env_var() {
    assert_eq!(
        is_hardcoded_denied("bash", "rm -rf $HOME"),
        Some("Destructive: removes home directory")
    );
}

#[test]
fn hardcoded_denied_rm_rf_current_dir() {
    assert_eq!(
        is_hardcoded_denied("bash", "rm -rf ."),
        Some("Destructive: removes current directory")
    );
}

#[test]
fn hardcoded_denied_rm_rf_parent_dir() {
    assert_eq!(
        is_hardcoded_denied("bash", "rm -rf .."),
        Some("Destructive: removes parent directory")
    );
}

#[test]
fn hardcoded_denied_sudo_rm_rf_wildcard() {
    assert_eq!(
        is_hardcoded_denied("bash", "sudo rm -rf /tmp/*"),
        Some("Destructive: root removal with wildcard")
    );
}

#[test]
fn hardcoded_denied_mkfs() {
    assert_eq!(
        is_hardcoded_denied("bash", "mkfs.ext4 /dev/sda1"),
        Some("Destructive: formats filesystem")
    );
}

#[test]
fn hardcoded_denied_dd_block_device() {
    assert_eq!(
        is_hardcoded_denied("bash", "dd if=/dev/zero of=/dev/sda"),
        Some("Destructive: writes to block device")
    );
}

#[test]
fn hardcoded_allowed_safe_command() {
    assert_eq!(is_hardcoded_denied("bash", "cargo test"), None);
}

#[test]
fn hardcoded_allowed_safe_rm() {
    assert_eq!(is_hardcoded_denied("bash", "rm -rf ~/Documents"), None);
}

#[test]
fn hardcoded_allowed_edit_tool() {
    assert_eq!(is_hardcoded_denied("edit", "src/main.rs"), None);
}

#[test]
fn hardcoded_allowed_read_tool() {
    assert_eq!(is_hardcoded_denied("read", "/etc/passwd"), None);
}

#[test]
fn hardcoded_denied_with_whitespace() {
    assert_eq!(
        is_hardcoded_denied("bash", "  rm -rf /  "),
        Some("Destructive: removes root filesystem")
    );
}

#[test]
fn hardcoded_denied_rm_rf_root_with_args() {
    assert_eq!(
        is_hardcoded_denied("bash", "rm -rf / --force"),
        Some("Destructive: removes root filesystem")
    );
}

#[test]
fn hardcoded_denied_mkfs_variants() {
    assert_eq!(
        is_hardcoded_denied("bash", "mkfs.btrfs /dev/sdb"),
        Some("Destructive: formats filesystem")
    );
}
