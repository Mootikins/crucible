//! File-system helpers that more than one crate needs.

use std::io::Write;
use std::path::Path;

/// Write `contents` to `path` so that only the owner can read it.
///
/// The API key, the session store, the provider secrets and the webhook
/// secrets all hold live credentials. One function decides how a credential
/// file is created, so no caller forgets the mode.
///
/// On unix the file is created with mode `0o600`. When the file already
/// exists with a looser mode, the function tightens the mode to `0o600`
/// before it writes the contents. Parent directories are created as needed.
pub fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    #[cfg(unix)]
    {
        use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};
        let mut file = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(path)?;
        // `mode()` applies only at creation. Tighten an existing file before
        // the secret reaches the disk.
        file.set_permissions(std::fs::Permissions::from_mode(0o600))?;
        file.write_all(contents)
    }
    #[cfg(not(unix))]
    {
        std::fs::File::create(path)?.write_all(contents)
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;

    fn mode_of(path: &Path) -> u32 {
        std::fs::metadata(path).unwrap().permissions().mode() & 0o777
    }

    #[test]
    fn new_file_is_owner_only() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("nested").join("secret");
        write_private(&path, b"s").unwrap();
        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"s");
    }

    #[test]
    fn existing_loose_file_is_tightened() {
        let dir = tempfile::TempDir::new().unwrap();
        let path = dir.path().join("secret");
        std::fs::write(&path, "old contents that are longer").unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o644)).unwrap();
        write_private(&path, b"new").unwrap();
        assert_eq!(mode_of(&path), 0o600);
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
    }
}
