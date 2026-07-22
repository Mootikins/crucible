//! Advisory single-daemon socket lock.
//!
//! Split out of `server/mod.rs` for the module-size budget; behavior is
//! unchanged.

use anyhow::Result;
#[cfg(unix)]
use tracing::warn;

/// Acquire an exclusive, non-blocking advisory lock on `<socket>.lock`.
///
/// Returns an error only when another process already holds the lock (i.e. a
/// daemon is running) — the caller should connect to it instead of binding.
/// Any other lock-infrastructure problem fails open (returns `Ok(None)`) so a
/// filesystem quirk can't wedge daemon startup.
#[cfg(unix)]
pub(super) fn acquire_socket_lock(socket_path: &std::path::Path) -> Result<Option<std::fs::File>> {
    use std::os::unix::io::AsRawFd;

    let lock_path = socket_path.with_extension("lock");
    let file = match std::fs::OpenOptions::new()
        .create(true)
        .read(true)
        .write(true)
        .truncate(false)
        .open(&lock_path)
    {
        Ok(f) => f,
        Err(e) => {
            warn!(?lock_path, error = %e, "could not open daemon lock file; proceeding without it");
            return Ok(None);
        }
    };

    let rc = unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) };
    if rc == 0 {
        return Ok(Some(file));
    }

    let err = std::io::Error::last_os_error();
    if err.raw_os_error() == Some(libc::EWOULDBLOCK) {
        anyhow::bail!(
            "another daemon already holds {:?}; connect to it instead of binding",
            lock_path
        );
    }
    warn!(?lock_path, error = %err, "flock failed for a non-contention reason; proceeding without the lock");
    Ok(None)
}

#[cfg(not(unix))]
pub(super) fn acquire_socket_lock(_socket_path: &std::path::Path) -> Result<Option<std::fs::File>> {
    Ok(None)
}
