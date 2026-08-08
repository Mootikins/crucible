use std::fs;
use std::path::{Path, PathBuf};

const SOCKET_FILE_NAME: &str = "crucible.sock";

/// Get the socket path for the daemon
///
/// Priority:
/// 1. `CRUCIBLE_SOCKET` environment variable (if set)
/// 2. `$XDG_RUNTIME_DIR/crucible.sock` (if XDG_RUNTIME_DIR is set)
/// 3. `<tmpdir>/crucible-<uid>/crucible.sock` — see [`fallback_socket_dir`]
pub fn socket_path() -> PathBuf {
    if let Ok(path) = std::env::var("CRUCIBLE_SOCKET") {
        return PathBuf::from(path);
    }
    dirs::runtime_dir()
        .unwrap_or_else(fallback_socket_dir)
        .join(SOCKET_FILE_NAME)
}

/// Directory holding the socket when `XDG_RUNTIME_DIR` is unset — the normal
/// case for a systemd system service or a container.
///
/// This used to be the temp dir itself, so the daemon's entire unauthenticated
/// RPC surface sat at `/tmp/crucible.sock`: a name every local user can predict
/// and reach. A per-uid subdirectory removes both halves — the name is no longer
/// shared between users, and a directory we created at 0700 cannot be traversed
/// by anyone else.
pub fn fallback_socket_dir() -> PathBuf {
    per_uid_socket_dir(&std::env::temp_dir(), current_uid())
}

fn per_uid_socket_dir(base: &Path, uid: u32) -> PathBuf {
    base.join(format!("crucible-{uid}"))
}

/// The uid this process runs as.
#[cfg(unix)]
pub fn current_uid() -> u32 {
    // `geteuid` is in the libc every unix target already links, and is
    // infallible. Declaring it is cheaper than growing this crate's dependency
    // graph for one syscall.
    extern "C" {
        fn geteuid() -> u32;
    }
    unsafe { geteuid() }
}

#[cfg(not(unix))]
pub fn current_uid() -> u32 {
    0
}

/// Why an existing entry is not fit to hold the daemon socket.
///
/// Public because how loud each reason should be depends on *who chose the
/// path*: a squat on the name we pick ourselves has no safe continuation, while
/// the same finding on an operator-supplied directory is their call to make.
/// The caller decides; this type only reports.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SocketDirRefusal {
    /// A symlink. Never followed — see [`ensure_private_socket_dir`].
    Symlink,
    /// Exists, but is not a directory.
    NotADirectory,
    /// A directory owned by some other uid.
    ForeignOwner(u32),
    /// Ours, but carries group or other permission bits (the mode is reported).
    GroupOrOtherAccess(u32),
}

impl std::fmt::Display for SocketDirRefusal {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Symlink => write!(f, "is a symlink, which its owner can repoint at any time"),
            Self::NotADirectory => write!(f, "exists but is not a directory"),
            Self::ForeignOwner(uid) => write!(f, "is owned by uid {uid}, not by this process"),
            Self::GroupOrOtherAccess(mode) => {
                write!(f, "has mode {mode:04o}, which other local users can enter")
            }
        }
    }
}

/// Make sure `dir` is a directory only this user can reach.
///
/// `Ok(Ok(()))` — it is ours and private (we created it, or it already was).
/// `Ok(Err(refusal))` — there is an entry we will not bind under.
/// `Err(io)` — we could not find out; the caller turns this into a startup error.
///
/// **This function never modifies an existing entry.** It does not chmod, unlink
/// or rename. `fs::set_permissions` follows symlinks, so "repair a directory
/// that is too permissive" is an arbitrary-chmod primitive the moment the entry
/// under a predictable name is not the one we stat'd — exactly the situation
/// this check exists to detect. We only ever *create*, and we create narrow.
#[cfg(unix)]
pub fn ensure_private_socket_dir(dir: &Path) -> std::io::Result<Result<(), SocketDirRefusal>> {
    use std::io::ErrorKind;
    use std::os::unix::fs::DirBuilderExt;

    // Two passes at most. A concurrent daemon can win the create race between
    // our `lstat` and our `mkdir`, in which case its directory is the one to
    // verify. Bounded so an attacker toggling the entry cannot spin us.
    for _ in 0..2 {
        // `symlink_metadata`, never `metadata`: `metadata` FOLLOWS symlinks, so
        // a symlink planted at this predictable name would be judged by its
        // TARGET's owner and mode. Aiming it at any 0700 directory we own passes
        // every ownership check while the attacker still owns the link itself —
        // /tmp's sticky bit stops us replacing it — leaving them free to repoint
        // it at their own listener afterwards.
        match fs::symlink_metadata(dir) {
            Ok(meta) => return Ok(refusal_for(&meta, current_uid()).map_or(Ok(()), Err)),
            Err(e) if e.kind() == ErrorKind::NotFound => {
                if let Some(parent) = dir.parent() {
                    fs::create_dir_all(parent)?;
                }
                // Created *with* the mode in one step: create-then-chmod would
                // leave it world-traversable in between. umask can only clear
                // bits, so the result is never wider than 0700.
                match fs::DirBuilder::new().mode(0o700).create(dir) {
                    Ok(()) => return Ok(Ok(())),
                    // Someone got there first. Go back and verify what they made.
                    Err(e) if e.kind() == ErrorKind::AlreadyExists => continue,
                    Err(e) => return Err(e),
                }
            }
            Err(e) => return Err(e),
        }
    }
    // Both passes saw the entry vanish and reappear. Refuse rather than loop:
    // something is actively churning the name.
    Ok(Err(SocketDirRefusal::NotADirectory))
}

#[cfg(not(unix))]
pub fn ensure_private_socket_dir(dir: &Path) -> std::io::Result<Result<(), SocketDirRefusal>> {
    fs::create_dir_all(dir)?;
    Ok(Ok(()))
}

/// The verdict on an already-`lstat`ed entry, split out so the hostile cases are
/// testable: an unprivileged test process cannot create a directory owned by
/// another uid.
#[cfg(unix)]
fn refusal_for(meta: &fs::Metadata, our_uid: u32) -> Option<SocketDirRefusal> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    if meta.file_type().is_symlink() {
        return Some(SocketDirRefusal::Symlink);
    }
    if !meta.is_dir() {
        return Some(SocketDirRefusal::NotADirectory);
    }
    if meta.uid() != our_uid {
        return Some(SocketDirRefusal::ForeignOwner(meta.uid()));
    }
    let mode = meta.permissions().mode() & 0o777;
    if mode & 0o077 != 0 {
        return Some(SocketDirRefusal::GroupOrOtherAccess(mode));
    }
    None
}

pub fn remove_socket(path: &Path) {
    let _ = fs::remove_file(path);
}

#[cfg(all(test, unix))]
mod private_socket_dir_tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    fn lstat(path: &Path) -> fs::Metadata {
        fs::symlink_metadata(path).unwrap()
    }

    #[test]
    fn the_fallback_socket_dir_is_per_uid() {
        let base = Path::new("/tmp");
        assert_eq!(
            per_uid_socket_dir(base, 1000),
            PathBuf::from("/tmp/crucible-1000")
        );
        assert_ne!(
            per_uid_socket_dir(base, 1000),
            per_uid_socket_dir(base, 1001)
        );
    }

    #[test]
    fn a_missing_socket_dir_is_created_reachable_only_by_its_owner() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("crucible-1000");

        ensure_private_socket_dir(&dir)
            .expect("no io error")
            .expect("creating a fresh socket dir must succeed");

        let mode = lstat(&dir).permissions().mode() & 0o777;
        assert_eq!(mode & 0o077, 0, "socket dir mode was {mode:o}");
    }

    #[test]
    fn an_already_private_socket_dir_is_accepted() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("crucible-1000");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();

        ensure_private_socket_dir(&dir)
            .expect("no io error")
            .expect("a 0700 dir we own must be accepted");
    }

    /// The whole point of "refuse, never repair": a too-permissive directory is
    /// *reported*, and the entry on disk is left exactly as it was.
    ///
    /// Repairing it looks harmless and is not. `fs::set_permissions` follows
    /// symlinks, so a chmod here is an arbitrary-chmod-to-0700 primitive against
    /// whatever the name resolves to at the moment of the call — which, on a
    /// path another user can swap, is not what we just stat'ed.
    #[test]
    fn a_socket_dir_that_other_users_can_enter_is_refused_and_left_untouched() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("crucible-1000");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o755)).unwrap();

        let refusal = ensure_private_socket_dir(&dir)
            .expect("no io error")
            .expect_err("a group/other-reachable dir must be refused");
        assert_eq!(refusal, SocketDirRefusal::GroupOrOtherAccess(0o755));

        let mode = lstat(&dir).permissions().mode() & 0o777;
        assert_eq!(mode, 0o755, "the directory must not be modified");
    }

    /// The squat that defeats an ownership check written with `fs::metadata`:
    /// the link is the attacker's, the target is ours and 0700, and following
    /// the link hands the check the target's credentials. Whoever owns the link
    /// can repoint it at their own listener at any later moment, so the entry
    /// type alone has to be disqualifying — and the link itself must survive the
    /// call, because unlinking a foreign-owned entry is its own primitive.
    #[test]
    fn a_symlinked_socket_dir_is_refused_even_when_it_points_at_a_dir_we_own() {
        let tmp = TempDir::new().unwrap();
        let target = tmp.path().join("innocent");
        fs::create_dir(&target).unwrap();
        fs::set_permissions(&target, fs::Permissions::from_mode(0o700)).unwrap();

        let link = tmp.path().join("crucible-1000");
        std::os::unix::fs::symlink(&target, &link).unwrap();

        let refusal = ensure_private_socket_dir(&link)
            .expect("no io error")
            .expect_err("a symlink at the socket dir name must be refused");
        assert_eq!(refusal, SocketDirRefusal::Symlink);
        assert!(
            lstat(&link).file_type().is_symlink(),
            "the symlink must still be there: we report, we do not repair"
        );
    }

    /// A directory owned by a *different* uid cannot be created by an
    /// unprivileged test process, so what is pinned here is the predicate
    /// `ensure_private_socket_dir` consults after the `lstat`.
    #[test]
    fn a_socket_dir_owned_by_another_user_is_refused() {
        let tmp = TempDir::new().unwrap();
        let dir = tmp.path().join("crucible-1000");
        fs::create_dir(&dir).unwrap();
        fs::set_permissions(&dir, fs::Permissions::from_mode(0o700)).unwrap();
        let meta = lstat(&dir);

        assert_eq!(refusal_for(&meta, current_uid()), None);
        assert_eq!(
            refusal_for(&meta, current_uid() ^ 1),
            Some(SocketDirRefusal::ForeignOwner(current_uid())),
            "a 0700 dir belonging to somebody else must still be refused"
        );
    }

    #[test]
    fn a_socket_path_that_is_not_a_directory_is_refused() {
        let tmp = TempDir::new().unwrap();
        let file = tmp.path().join("crucible-1000");
        fs::write(&file, "").unwrap();

        let refusal = ensure_private_socket_dir(&file)
            .expect("no io error")
            .expect_err("a non-directory must be refused");
        assert_eq!(refusal, SocketDirRefusal::NotADirectory);
        assert!(file.exists(), "the file must not be unlinked");
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_socket_path_not_empty() {
        let path = socket_path();
        assert!(path.to_string_lossy().contains("crucible.sock"));
    }

    #[test]
    fn test_remove_socket() {
        let tmp = TempDir::new().unwrap();
        let sock_path = tmp.path().join("test.sock");
        fs::write(&sock_path, "").unwrap();
        assert!(sock_path.exists());
        remove_socket(&sock_path);
        assert!(!sock_path.exists());
        remove_socket(&sock_path); // Should not panic
    }
}
