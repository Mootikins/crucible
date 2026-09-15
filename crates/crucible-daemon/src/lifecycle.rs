pub use crucible_core::protocol::{remove_socket, socket_path};

use anyhow::Result;

/// SIGTERM and SIGINT handlers, installed and already buffering.
///
/// **Install this before the daemon binds its socket.** The socket becomes
/// connectable early in the bind, while the server is still constructing
/// everything behind it, so a daemon that installed its handlers afterwards
/// was reachable for hundreds of milliseconds with SIGTERM's default
/// disposition still in force — `kill` at that moment killed it outright,
/// mid-write. `sigterm_shuts_the_daemon_down_cleanly` caught exactly that,
/// failing on the runs where the daemon came up in 40ms.
///
/// Tokio buffers a signal that arrives before anything awaits it, so a SIGTERM
/// during boot is not lost: [`Self::forward_to`] delivers it as soon as there
/// is a server to stop.
///
/// **Only a process whose whole job is to be the daemon may install these.** A
/// signal handler is process-global: `cru --standalone` runs a daemon inside
/// the same process as the TUI, and installing one there would take the TUI's
/// Ctrl-C away from it.
pub struct ShutdownSignals {
    #[cfg(unix)]
    sigterm: tokio::signal::unix::Signal,
    #[cfg(unix)]
    sigint: tokio::signal::unix::Signal,
}

impl ShutdownSignals {
    /// Put the handlers in place. They are in force when this returns.
    pub fn install() -> Result<Self> {
        #[cfg(unix)]
        {
            use tokio::signal::unix::{signal, SignalKind};
            Ok(Self {
                sigterm: signal(SignalKind::terminate())?,
                sigint: signal(SignalKind::interrupt())?,
            })
        }

        #[cfg(not(unix))]
        {
            Ok(Self {})
        }
    }

    /// Shut the running server down on the first signal.
    pub fn forward_to(
        self,
        shutdown_tx: tokio::sync::broadcast::Sender<()>,
    ) -> tokio::task::JoinHandle<()> {
        tokio::spawn(async move {
            self.recv().await;
            // A closed channel means `run()` already returned, which is the
            // same outcome this asks for.
            let _ = shutdown_tx.send(());
        })
    }

    async fn recv(mut self) {
        #[cfg(unix)]
        {
            tokio::select! {
                _ = self.sigterm.recv() => tracing::info!("Received SIGTERM"),
                _ = self.sigint.recv() => tracing::info!("Received SIGINT"),
            }
        }

        #[cfg(not(unix))]
        {
            // On non-Unix systems, just wait for Ctrl+C.
            let _ = tokio::signal::ctrl_c().await;
            tracing::info!("Received shutdown signal");
        }
    }
}
