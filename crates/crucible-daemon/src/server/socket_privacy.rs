//! Making the daemon's front door reachable only by the user who owns it.
//!
//! Three separate guards, split out of `server/mod.rs` for the module-size
//! budget (same reason as `socket_lock`), behavior unchanged:
//!
//! - the directory the socket lives in is verified before anything opens a file
//!   in it ([`prepare_socket_dir`]);
//! - the socket itself is bound at 0600 ([`bind_private_listener`]);
//! - every accepted connection's `SO_PEERCRED` uid must be ours
//!   (`core::peer_accepted`, called from `handle_client`).
//!
//! The governing rule for the first one is **refuse, never repair**. Anything
//! that mutates an existing entry at a predictable path — `chmod`, `unlink`,
//! `rename` — is a primitive an attacker can aim, because those calls take a
//! path and resolve it again after we looked at it.

use anyhow::Result;
use std::path::Path;
use tokio::net::UnixListener;
use tracing::{debug, warn};

/// Run `f` with the process umask set to `mask`, then restore it.
///
/// umask is process-global, so the save/restore pair has to be serialized
/// against every other user of it in this process or two interleaving windows
/// can restore a wide value in the middle of the other's — which an in-process
/// harness binding two `Server`s really can do. The mutex makes the pair atomic.
///
/// Not reentrant, and it must not become so: it is a plain `std::sync::Mutex`,
/// so nesting one window inside another deadlocks. Tests that need a known
/// *ambient* umask around a bind set it directly instead.
///
/// Residual, stated plainly: this makes the *windows* exclusive, not the umask
/// private. A thread creating an unrelated file during the window still sees
/// `mask`. That is the tightening direction and it lasts microseconds.
#[cfg(unix)]
fn with_umask<T>(mask: libc::mode_t, f: impl FnOnce() -> T) -> T {
    static UMASK_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    // A panic inside a previous window poisons the lock; the umask was already
    // restored by then either way, so the guard is still usable.
    let _guard = UMASK_LOCK.lock().unwrap_or_else(|e| e.into_inner());
    let previous = unsafe { libc::umask(mask) };
    let out = f();
    unsafe { libc::umask(previous) };
    out
}

/// Bind the RPC socket so that only this user can connect to it.
///
/// `bind` + `chmod` has a real race: between the two calls the socket is already
/// accepting connections at the umask-derived mode (0755 under the usual 022).
/// The kernel applies umask *inside* `bind`, so narrowing it there means the
/// socket never exists at a wider mode.
///
/// There is deliberately no chmod afterwards. `set_permissions` takes a path and
/// follows symlinks, so a post-bind chmod on a path in a directory we may only
/// have *warned* about is an arbitrary-chmod primitive, and `fchmod` on the
/// listener fd is not an alternative: on Linux it returns success and changes
/// nothing, because it acts on the sockfs inode rather than the filesystem
/// entry — a control that would pass its own test while doing nothing.
#[cfg(unix)]
pub(super) fn bind_private_listener(path: &Path) -> std::io::Result<UnixListener> {
    with_umask(0o177, || UnixListener::bind(path))
}

#[cfg(not(unix))]
pub(super) fn bind_private_listener(path: &Path) -> std::io::Result<UnixListener> {
    UnixListener::bind(path)
}

/// Prepare the directory the socket will live in. Reports; never repairs.
///
/// `fallback_dir` is the name *we* pick (`<tmp>/crucible-<uid>`, used when
/// `XDG_RUNTIME_DIR` is unset — a systemd system service or a container). An
/// entry there that is not our own private directory is another local user
/// squatting the daemon's front door, and binding into a directory somebody else
/// controls is silent compromise: they unlink our socket, put their listener at
/// the same name, and every client connects to it. There is no safe way to
/// continue, so that case stops startup.
///
/// This is not "refuse to start when `XDG_RUNTIME_DIR` is unset" — the fallback
/// itself stays fully supported, which is what containers need. Only a squatted
/// fallback is fatal.
///
/// Any other directory came from the operator (`CRUCIBLE_SOCKET`,
/// `XDG_RUNTIME_DIR`). A shared one is their deliberate choice and has to keep
/// working, with the 0600 socket mode and the peer check as the guards there. We
/// say so and continue — we do not chmod, unlink or rename a path an operator
/// pointed us at. The `create_dir_all` in those arms is the call this function
/// replaced, unchanged: it still follows symlinks, so a dangling link at an
/// operator path still materializes its target. That is pre-existing behavior on
/// a path the operator supplied, deliberately not widened and deliberately not
/// "fixed" here into an unlink.
pub(super) fn prepare_socket_dir(dir: &Path, fallback_dir: &Path) -> Result<()> {
    use crucible_core::protocol::lifecycle::{ensure_private_socket_dir, SocketDirRefusal};

    match ensure_private_socket_dir(dir)? {
        Ok(()) => Ok(()),
        Err(refusal) if dir == fallback_dir => anyhow::bail!(
            "refusing to bind the daemon socket: {} {refusal}. \
             Set CRUCIBLE_SOCKET or XDG_RUNTIME_DIR to a directory you own, \
             or remove {}.",
            dir.display(),
            dir.display()
        ),
        // Ordinary for operator-chosen paths — `/var/run` IS a symlink to `/run`
        // on Fedora and Debian — so this is not worth crying wolf over.
        Err(refusal @ (SocketDirRefusal::Symlink | SocketDirRefusal::NotADirectory)) => {
            debug!(dir = %dir.display(), %refusal, "operator-chosen socket directory");
            std::fs::create_dir_all(dir)?;
            Ok(())
        }
        Err(refusal) => {
            warn!(
                dir = %dir.display(),
                %refusal,
                "the daemon socket directory is reachable by other local users"
            );
            std::fs::create_dir_all(dir)?;
            Ok(())
        }
    }
}

#[cfg(all(test, unix))]
mod tests {
    use super::*;
    use crate::server::core::daemon_uid;
    use crate::server::Server;
    use std::os::unix::fs::PermissionsExt;
    use std::time::Duration;
    use tempfile::TempDir;
    use tokio::io::AsyncWriteExt;
    use tokio::net::UnixStream;

    /// Pin the *ambient* umask around `f`.
    ///
    /// Deliberately not `with_umask`: the code under test takes that lock
    /// itself, and `std::sync::Mutex` is not reentrant, so nesting deadlocks —
    /// it did, before this helper existed. Setting it raw here is safe because
    /// nextest gives every test its own process, and what these tests need is
    /// the ambient value, not an exclusive window.
    fn with_ambient_umask<T>(mask: libc::mode_t, f: impl FnOnce() -> T) -> T {
        let previous = unsafe { libc::umask(mask) };
        let out = f();
        unsafe { libc::umask(previous) };
        out
    }

    /// Pin the socket mode against a *known* ambient umask.
    ///
    /// Asserting 0600 at whatever umask the harness inherited proves nothing:
    /// under a restrictive 0077 a plain `UnixListener::bind` also lands on 0600,
    /// so the assertion would be satisfied by the environment rather than by the
    /// code. Forcing the ambient umask to 0 makes 0600 reachable only by a bind
    /// that narrows the mode itself — measured: an unfixed bind under umask 0
    /// produces 0777.
    ///
    /// This drives the whole `Server::bind_with_plugin_config` path, not
    /// `bind_private_listener` in isolation, so deleting the call site fails it
    /// too. The bind is `async`, so it is driven on a current-thread runtime
    /// *inside* the sync umask window.
    #[test]
    fn a_bound_socket_is_reachable_only_by_its_owner() {
        let tmp = TempDir::new().unwrap();
        let sock = tmp.path().join("crucible.sock");
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        let server = with_ambient_umask(0, || {
            rt.block_on(Server::bind_with_data_home(&sock, tmp.path().to_path_buf()))
        })
        .expect("bind");

        let mode = std::fs::metadata(&sock).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "socket mode was {mode:o}, want 600");
        drop(server);
    }

    /// The squat is fatal only for the name *we* choose. An operator who points
    /// `CRUCIBLE_SOCKET`/`XDG_RUNTIME_DIR` somewhere shared keeps working — and
    /// either way the entry on disk is left exactly as we found it.
    #[test]
    fn a_squatted_fallback_dir_stops_the_bind_but_an_operator_dir_only_warns() {
        let tmp = TempDir::new().unwrap();
        let attacker_dir = tmp.path().join("attacker");
        std::fs::create_dir(&attacker_dir).unwrap();
        // A symlink is the squat that an ownership check written with
        // `fs::metadata` cannot see, and the one case that can never be repaired.
        let squatted = tmp.path().join("crucible-1000");
        std::os::unix::fs::symlink(&attacker_dir, &squatted).unwrap();

        let err = prepare_socket_dir(&squatted, &squatted)
            .expect_err("a squatted fallback dir must stop the daemon binding");
        assert!(
            err.to_string().contains("symlink"),
            "the refusal must say why: {err}"
        );

        prepare_socket_dir(&squatted, &tmp.path().join("some-other-fallback"))
            .expect("an operator-chosen dir must keep working");

        assert!(
            std::fs::symlink_metadata(&squatted)
                .unwrap()
                .file_type()
                .is_symlink(),
            "neither branch may unlink or replace an entry it did not create"
        );
    }

    /// The fallback directory we create ourselves is private, pinned against a
    /// known ambient umask for the same reason as the socket-mode test:
    /// `create_dir_all` also yields 0700 under an inherited umask of 0077.
    #[test]
    fn a_fresh_fallback_dir_is_created_private() {
        let tmp = TempDir::new().unwrap();
        let fallback = tmp.path().join("crucible-1000");

        with_ambient_umask(0, || prepare_socket_dir(&fallback, &fallback))
            .expect("creating our own fallback dir");

        let mode = std::fs::metadata(&fallback).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o700, "fallback dir mode was {mode:o}");
    }

    /// Drives the peer check through the real accept loop.
    ///
    /// Testing `peer_accepted` in isolation would pass just as happily with the
    /// call deleted from `handle_client`, which is how a control ends up wired
    /// but dormant. Spawning a second uid is not available to the suite, so the
    /// *expected* uid is moved instead: the server below authorizes a uid this
    /// process does not have, so our own connection is the unauthorized one and
    /// must be dropped before it can reach dispatch.
    ///
    /// Measured RED: with `authorized_uid` fully plumbed into `ServerContext`
    /// and `peer_accepted` defined but not called, this test received
    /// `{"jsonrpc":"2.0","id":1,"result":"pong"}`.
    #[tokio::test]
    async fn a_client_whose_uid_is_not_authorized_is_dropped_before_dispatch() {
        use tokio::io::AsyncReadExt;

        let tmp = TempDir::new().unwrap();
        let sock = tmp.path().join("crucible.sock");

        let mut server = Server::bind_with_data_home(&sock, tmp.path().to_path_buf())
            .await
            .expect("bind");
        // `daemon_uid() ^ 1` is a uid this process cannot be running as.
        server.set_authorized_uid(daemon_uid() ^ 1);
        let shutdown_tx = server.shutdown_handle();
        let task = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock).await.expect("connect");
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{}}\n")
            .await
            .expect("write");

        let mut buf = vec![0u8; 1024];
        let read = tokio::time::timeout(Duration::from_secs(2), client.read(&mut buf)).await;

        let _ = shutdown_tx.send(());
        let _ = task.await;

        match read {
            Ok(Ok(0)) => {}
            Ok(Ok(n)) => panic!(
                "an unauthorized peer was answered: {}",
                String::from_utf8_lossy(&buf[..n])
            ),
            Ok(Err(e)) => assert!(
                matches!(
                    e.kind(),
                    std::io::ErrorKind::ConnectionReset | std::io::ErrorKind::BrokenPipe
                ),
                "unexpected error for a refused peer: {e}"
            ),
            Err(_) => panic!("the daemon held an unauthorized connection open"),
        }
    }

    /// The other half: with the real uid authorized, the same request is served.
    /// Without this, `a_client_whose_uid_is_not_authorized_is_dropped_before_dispatch`
    /// would also pass if the daemon simply refused everyone.
    #[tokio::test]
    async fn a_client_running_as_the_daemons_uid_is_served() {
        use tokio::io::AsyncReadExt;

        let tmp = TempDir::new().unwrap();
        let sock = tmp.path().join("crucible.sock");

        let server = Server::bind_with_data_home(&sock, tmp.path().to_path_buf())
            .await
            .expect("bind");
        let shutdown_tx = server.shutdown_handle();
        let task = tokio::spawn(server.run());
        tokio::time::sleep(Duration::from_millis(50)).await;

        let mut client = UnixStream::connect(&sock).await.expect("connect");
        client
            .write_all(b"{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\",\"params\":{}}\n")
            .await
            .expect("write");

        let mut buf = vec![0u8; 1024];
        let n = tokio::time::timeout(Duration::from_secs(5), client.read(&mut buf))
            .await
            .expect("the daemon must answer its own user")
            .expect("read");

        let _ = shutdown_tx.send(());
        let _ = task.await;

        assert!(n > 0, "an authorized peer got no response");
        let response = String::from_utf8_lossy(&buf[..n]);
        assert!(
            response.contains("\"id\":1"),
            "an authorized peer must be dispatched, got: {response}"
        );
    }
}
