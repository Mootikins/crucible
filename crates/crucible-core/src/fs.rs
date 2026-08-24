//! File-system helpers that more than one crate needs.

use std::io::Write;
use std::path::Path;

/// Write `contents` to `path` so that only the owner can read it.
///
/// The API key, the session store, the provider secrets and the webhook
/// secrets all hold live credentials. One function decides how a credential
/// file is created, so no caller forgets the mode.
///
/// The contents go to a sibling temporary file first. A rename then moves
/// the file into place, so a reader never sees a truncated file.
///
/// On unix the file is created with mode `0o600`. Parent directories are
/// created as needed.
pub fn write_private(path: &Path, contents: &[u8]) -> std::io::Result<()> {
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    std::fs::create_dir_all(parent)?;
    let file_name = path
        .file_name()
        .ok_or_else(|| std::io::Error::other(format!("{}: no file name", path.display())))?;
    // The counter keeps two threads of one process apart. The process id
    // alone is not enough: a concurrent write is the case this guards.
    static NEXT: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let seq = NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let mut tmp_name = std::ffi::OsString::from(".");
    tmp_name.push(file_name);
    tmp_name.push(format!(".{}.{seq}.tmp", std::process::id()));
    let tmp = parent.join(tmp_name);

    let result = write_new_private(&tmp, contents).and_then(|()| std::fs::rename(&tmp, path));
    if result.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    result
}

fn write_new_private(tmp: &Path, contents: &[u8]) -> std::io::Result<()> {
    #[cfg(unix)]
    let mut file = {
        use std::os::unix::fs::OpenOptionsExt;
        std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(tmp)?
    };
    #[cfg(not(unix))]
    let mut file = std::fs::File::create(tmp)?;
    file.write_all(contents)?;
    file.sync_all()
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
