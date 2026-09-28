//! Shared test fixture for rpc_integration tests.
//!
//! A thin wrapper over the shared in-process daemon harness
//! (`crucible_daemon::test_support::InProcessDaemon`), keeping the public
//! `socket_path` field this suite's submodules read directly.

use crate::common::{InProcessDaemon, InProcessDaemonBuilder};
use anyhow::Result;
use std::path::PathBuf;

/// Test fixture that starts a real daemon server for integration testing
pub struct TestServer {
    inner: InProcessDaemon,
    pub socket_path: PathBuf,
}

impl TestServer {
    pub async fn start() -> Result<Self> {
        // One registered kiln named `kiln`, directly under the data home
        // (not under a `kilns/` subdirectory): `bases.rs` derives the kiln's
        // path from the socket's parent rather than through an accessor.
        let builder = InProcessDaemonBuilder::new()?;
        let kiln = builder.data_home().join("kiln");
        let inner = builder.with_kiln_at("kiln", kiln).start().await?;
        let socket_path = inner.socket_path().to_path_buf();
        Ok(Self { inner, socket_path })
    }

    pub async fn shutdown(self) {
        self.inner.shutdown().await;
    }
}
