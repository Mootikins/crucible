//! When an `accept()` error may be retried immediately, and what to do when it
//! may not.
//!
//! Split from `server/mod.rs` for the 1000-line module budget.

/// How long the accept loop waits after an accept error it cannot retry
/// immediately. Long enough to turn a spin into ten log lines a second, short
/// enough that recovery is not perceptible to whoever is trying to connect.
pub(super) const ACCEPT_ERROR_BACKOFF: std::time::Duration = std::time::Duration::from_millis(100);

/// Whether an `accept()` error is a failure of *that connection* rather than of
/// the listener's resources.
///
/// The distinction decides whether retrying immediately is safe. These four mean
/// one peer went away between the kernel queueing the connection and us taking
/// it, so the next `accept()` is a fresh question and will park if nothing is
/// waiting. Everything else — fd exhaustion (`EMFILE`/`ENFILE`), `ENOMEM`,
/// `ENOBUFS` — is a condition that is still true on the next call and returns
/// synchronously, which is what makes an immediate retry a busy loop.
///
/// Matched on `ErrorKind` rather than `raw_os_error` deliberately: the errno
/// values for the exhaustion cases have no stable `ErrorKind` (they surface as
/// `Uncategorized`, which is unstable to name), so the safe direction is to
/// enumerate the *retryable* kinds and back off on anything unrecognised.
pub(super) fn accept_error_is_transient(e: &std::io::Error) -> bool {
    matches!(
        e.kind(),
        std::io::ErrorKind::ConnectionAborted
            | std::io::ErrorKind::ConnectionReset
            | std::io::ErrorKind::ConnectionRefused
            | std::io::ErrorKind::Interrupted
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Error, ErrorKind};

    #[test]
    fn a_peer_that_vanished_is_retried_at_once() {
        for kind in [
            ErrorKind::ConnectionAborted,
            ErrorKind::ConnectionReset,
            ErrorKind::ConnectionRefused,
            ErrorKind::Interrupted,
        ] {
            assert!(
                accept_error_is_transient(&Error::from(kind)),
                "{kind:?} is one connection's failure, so the next accept() is a fresh question"
            );
        }
    }

    #[test]
    fn a_resource_error_is_not_retried_at_once() {
        // The ones that matter are fd exhaustion, which has no stable `ErrorKind`
        // and arrives as the raw errno: EMFILE (24) per-process, ENFILE (23)
        // system-wide. Both are still true on the next call and return without
        // registering readiness, which is what turns an immediate retry into a
        // busy loop. Constructed from raw errno precisely because that is how the
        // kernel delivers them.
        for (errno, name) in [
            (24, "EMFILE"),
            (23, "ENFILE"),
            (12, "ENOMEM"),
            (105, "ENOBUFS"),
        ] {
            let err = Error::from_raw_os_error(errno);
            assert!(
                !accept_error_is_transient(&err),
                "{name} (errno {errno}, kind {:?}) must back off, not spin",
                err.kind()
            );
        }
        // And anything unrecognised backs off too — the safe direction.
        assert!(!accept_error_is_transient(&Error::from(ErrorKind::Other)));
    }
}
