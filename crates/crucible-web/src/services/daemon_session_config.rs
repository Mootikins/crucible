//! The nine `session.{set,get}_*` config knobs the web could not reach.
//!
//! Split from `daemon.rs` along the same seam as `daemon_review` and
//! `daemon_plugins`: `daemon.rs` is 1431 lines and in `SIZE_LEDGER`, which only
//! shrinks, so eighteen more wrappers had to land somewhere else.
//!
//! Every one of these is the same six lines — clone the session id, name the RPC
//! method for `call_with_reconnect`'s label, forward to the typed
//! `rpc_client` method. They are wrappers rather than direct calls because
//! `call_with_reconnect` is what survives a daemon restart mid-session; a
//! handler calling the client directly would surface a broken pipe to the
//! browser.
//!
//! **All eighteen are idempotent setters and readers**, so `call_with_reconnect`
//! (which may replay a call the daemon already executed) is safe here — unlike
//! the four `review.*` writes next door, which take `call_once` for exactly that
//! reason.
//!
//! Parameter names deliberately mirror the client's, including where they do NOT
//! match the knob: `session_set_execution_timeout` takes `timeout_secs`, and that
//! is the wire field name too.

use super::daemon::ReconnectingDaemon;

impl ReconnectingDaemon {
    // ── Context ───────────────────────────────────────────────────────────

    pub async fn session_set_context_budget(
        &self,
        session_id: &str,
        context_budget: Option<usize>,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.set_context_budget", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move {
                daemon
                    .session_set_context_budget(&session_id, context_budget)
                    .await
            })
        })
        .await
    }

    pub async fn session_get_context_budget(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<usize>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_context_budget", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_context_budget(&session_id).await })
        })
        .await
    }

    pub async fn session_set_context_window(
        &self,
        session_id: &str,
        context_window: Option<usize>,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.set_context_window", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move {
                daemon
                    .session_set_context_window(&session_id, context_window)
                    .await
            })
        })
        .await
    }

    pub async fn session_get_context_window(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<usize>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_context_window", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_context_window(&session_id).await })
        })
        .await
    }

    pub async fn session_set_autocompact_threshold(
        &self,
        session_id: &str,
        threshold: Option<f32>,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.set_autocompact_threshold", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move {
                daemon
                    .session_set_autocompact_threshold(&session_id, threshold)
                    .await
            })
        })
        .await
    }

    pub async fn session_get_autocompact_threshold(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<f32>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_autocompact_threshold", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_autocompact_threshold(&session_id).await })
        })
        .await
    }

    // ── Execution ─────────────────────────────────────────────────────────

    pub async fn session_set_max_iterations(
        &self,
        session_id: &str,
        max_iterations: Option<u32>,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.set_max_iterations", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move {
                daemon
                    .session_set_max_iterations(&session_id, max_iterations)
                    .await
            })
        })
        .await
    }

    pub async fn session_get_max_iterations(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<u32>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_max_iterations", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_max_iterations(&session_id).await })
        })
        .await
    }

    /// `timeout_secs`, not `execution_timeout`: the knob is
    /// `session.set_execution_timeout` but its wire field has always been
    /// `timeout_secs`, recorded deliberately in the daemon's `CONFIG_METHODS`
    /// table. Naming the parameter after the knob is how the value gets
    /// silently dropped.
    pub async fn session_set_execution_timeout(
        &self,
        session_id: &str,
        timeout_secs: Option<u64>,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.set_execution_timeout", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move {
                daemon
                    .session_set_execution_timeout(&session_id, timeout_secs)
                    .await
            })
        })
        .await
    }

    pub async fn session_get_execution_timeout(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<u64>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_execution_timeout", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_execution_timeout(&session_id).await })
        })
        .await
    }

    pub async fn session_set_validation_retries(
        &self,
        session_id: &str,
        retries: u32,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.set_validation_retries", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move {
                daemon
                    .session_set_validation_retries(&session_id, retries)
                    .await
            })
        })
        .await
    }

    pub async fn session_get_validation_retries(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<u32>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_validation_retries", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_validation_retries(&session_id).await })
        })
        .await
    }

    // ── Prompt and enum-valued knobs ──────────────────────────────────────

    pub async fn session_set_context_strategy(
        &self,
        session_id: &str,
        strategy: &str,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        let strategy = strategy.to_string();
        self.call_with_reconnect("session.set_context_strategy", move |daemon| {
            let session_id = session_id.clone();
            let strategy = strategy.clone();
            Box::pin(async move {
                daemon
                    .session_set_context_strategy(&session_id, &strategy)
                    .await
            })
        })
        .await
    }

    pub async fn session_get_context_strategy(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_context_strategy", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_context_strategy(&session_id).await })
        })
        .await
    }

    pub async fn session_set_output_validation(
        &self,
        session_id: &str,
        validation: &str,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        let validation = validation.to_string();
        self.call_with_reconnect("session.set_output_validation", move |daemon| {
            let session_id = session_id.clone();
            let validation = validation.clone();
            Box::pin(async move {
                daemon
                    .session_set_output_validation(&session_id, &validation)
                    .await
            })
        })
        .await
    }

    pub async fn session_get_output_validation(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_output_validation", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_output_validation(&session_id).await })
        })
        .await
    }

    pub async fn session_set_system_prompt(
        &self,
        session_id: &str,
        prompt: &str,
    ) -> anyhow::Result<()> {
        let session_id = session_id.to_string();
        let prompt = prompt.to_string();
        self.call_with_reconnect("session.set_system_prompt", move |daemon| {
            let session_id = session_id.clone();
            let prompt = prompt.clone();
            Box::pin(async move { daemon.session_set_system_prompt(&session_id, &prompt).await })
        })
        .await
    }

    pub async fn session_get_system_prompt(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Option<String>> {
        let session_id = session_id.to_string();
        self.call_with_reconnect("session.get_system_prompt", move |daemon| {
            let session_id = session_id.clone();
            Box::pin(async move { daemon.session_get_system_prompt(&session_id).await })
        })
        .await
    }
}
