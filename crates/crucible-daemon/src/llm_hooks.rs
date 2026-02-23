//! LLM lifecycle hook chain for the daemon.
//!
//! Provides a simple sequential hook chain for pre-LLM and post-LLM processing.
//! This is SEPARATE from the Reactor — it's a direct call chain with fail-open
//! semantics and a 5-second timeout.
//!
//! # Design
//!
//! - `LlmHook` trait: async methods for pre/post LLM interception
//! - `LlmHookChain`: runs hooks sequentially with timeout protection
//! - Pre-LLM: hooks can modify context or cancel the call
//! - Post-LLM: hooks observe only; errors are logged but don't fail
//! - Fail-open: on timeout or error, original context is preserved

use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use crucible_core::events::llm_hook_context::{PostLlmContext, PreLlmContext, PreLlmResult};
use mlua::{Function, Value};
use tokio::sync::Mutex;
use tokio::time::timeout;
use tracing::warn;
use crate::agent_manager::SessionLuaState;

/// Async trait for LLM lifecycle hooks.
///
/// Implementors can intercept LLM calls before they happen (with the ability
/// to modify context or cancel) and observe responses after completion.
#[async_trait]
pub trait LlmHook: Send + Sync {
    /// Called before an LLM call. Can modify the context or cancel the call.
    ///
    /// Return `PreLlmResult::Continue(ctx)` to proceed (with possibly modified context),
    /// or `PreLlmResult::Cancel(reason)` to abort the LLM call.
    async fn on_pre_llm(&self, ctx: PreLlmContext) -> anyhow::Result<PreLlmResult>;

    /// Called after an LLM call completes. Observe-only in v1.
    ///
    /// Errors are logged but do not stop other hooks from running.
    async fn on_post_llm(&self, ctx: &PostLlmContext) -> anyhow::Result<()>;
}

/// Sequential hook chain for LLM lifecycle events.
///
/// Runs hooks in registration order with a 5-second timeout on the entire chain.
/// Fail-open: on timeout or error, logs a warning and returns the original context.
pub struct LlmHookChain {
    hooks: Vec<Box<dyn LlmHook + Send + Sync>>,
}

impl LlmHookChain {
    /// Create a new empty hook chain.
    pub fn new() -> Self {
        Self { hooks: Vec::new() }
    }

    /// Add a hook to the end of the chain.
    pub fn add_hook(&mut self, hook: Box<dyn LlmHook + Send + Sync>) {
        self.hooks.push(hook);
    }

    /// Register a Lua-backed hook that bridges to `on_pre_llm_call` / `on_post_llm_call`
    /// global functions in the session's Lua state.
    pub(crate) fn register_lua_hook(&mut self, lua_state: Arc<Mutex<SessionLuaState>>) {
        self.hooks.push(Box::new(LuaLlmHook::new(lua_state)));
    }

    /// Run pre-LLM hooks sequentially.
    ///
    /// Each hook receives the (possibly modified) context from the previous hook.
    /// If any hook returns `Cancel`, the chain short-circuits immediately.
    /// The entire chain is wrapped in a 5-second timeout; on timeout, the
    /// original context is returned unchanged (fail-open).
    pub async fn run_pre_llm(&self, ctx: PreLlmContext) -> PreLlmResult {
        if self.hooks.is_empty() {
            return PreLlmResult::Continue(ctx);
        }

        let original_ctx = ctx.clone();

        let result = timeout(Duration::from_secs(5), self.run_pre_llm_inner(ctx)).await;

        match result {
            Ok(pre_result) => pre_result,
            Err(_) => {
                warn!("LlmHookChain timed out after 5s, proceeding with original context");
                PreLlmResult::Continue(original_ctx)
            }
        }
    }

    async fn run_pre_llm_inner(&self, mut ctx: PreLlmContext) -> PreLlmResult {
        for hook in &self.hooks {
            let snapshot = ctx.clone();
            match hook.on_pre_llm(ctx).await {
                Ok(PreLlmResult::Continue(next_ctx)) => {
                    ctx = next_ctx;
                }
                Ok(PreLlmResult::Cancel(reason)) => {
                    return PreLlmResult::Cancel(reason);
                }
                Err(e) => {
                    warn!("LlmHook pre_llm error: {e:#}, proceeding with current context");
                    return PreLlmResult::Continue(snapshot);
                }
            }
        }
        PreLlmResult::Continue(ctx)
    }

    /// Run post-LLM hooks sequentially.
    ///
    /// All hooks run regardless of individual errors (errors are logged).
    /// The entire chain is wrapped in a 5-second timeout; on timeout,
    /// a warning is logged and execution returns Ok(()).
    pub async fn run_post_llm(&self, ctx: &PostLlmContext) {
        if self.hooks.is_empty() {
            return;
        }

        let result = timeout(Duration::from_secs(5), self.run_post_llm_inner(ctx)).await;

        if result.is_err() {
            warn!("LlmHookChain timed out after 5s, proceeding with original context");
        }
    }

    async fn run_post_llm_inner(&self, ctx: &PostLlmContext) {
        for hook in &self.hooks {
            if let Err(e) = hook.on_post_llm(ctx).await {
                warn!("LlmHook post_llm error: {e:#}");
            }
        }
    }
}

/// LLM lifecycle hook that bridges to Lua plugin functions.
///
/// Calls Lua global functions `on_pre_llm_call(ctx_json)` and `on_post_llm_call(ctx_json)`
/// if they are defined. Missing functions are treated as pass-through (no error).
///
/// The JSON context deliberately excludes `context_messages` — only `prompt`,
/// `model`, `session_id`, and `system_prompt` are exposed to Lua.
pub(crate) struct LuaLlmHook {
    lua_state: Arc<Mutex<SessionLuaState>>,
}

impl LuaLlmHook {
    pub(crate) fn new(lua_state: Arc<Mutex<SessionLuaState>>) -> Self {
        Self { lua_state }
    }
}

#[async_trait]
impl LlmHook for LuaLlmHook {
    async fn on_pre_llm(&self, ctx: PreLlmContext) -> anyhow::Result<PreLlmResult> {
        let state = self.lua_state.lock().await;

        // Check if function exists in Lua globals
        let func: Result<Function, _> = state.lua.globals().get("on_pre_llm_call");
        let func = match func {
            Ok(f) => f,
            Err(_) => return Ok(PreLlmResult::Continue(ctx)),
        };

        // Serialize context to JSON (excluding context_messages per design)
        let ctx_json = serde_json::json!({
            "prompt": ctx.prompt,
            "model": ctx.model,
            "session_id": ctx.session_id,
            "system_prompt": ctx.system_prompt,
        });
        let json_str = serde_json::to_string(&ctx_json)?;

        // Call Lua function
        let result: Value = func.call(json_str)?;

        // Parse return: if table has cancel=true, return Cancel
        if let Value::Table(ref t) = result {
            if let Ok(true) = t.get::<bool>("cancel") {
                let reason: String = t
                    .get::<String>("reason")
                    .unwrap_or_else(|_| "cancelled by Lua hook".to_string());
                return Ok(PreLlmResult::Cancel(reason));
            }
        }

        // Pass-through on any other return or parse failure
        Ok(PreLlmResult::Continue(ctx))
    }

    async fn on_post_llm(&self, ctx: &PostLlmContext) -> anyhow::Result<()> {
        let state = self.lua_state.lock().await;

        // Check if function exists in Lua globals
        let func: Result<Function, _> = state.lua.globals().get("on_post_llm_call");
        let func = match func {
            Ok(f) => f,
            Err(_) => return Ok(()),
        };

        // Serialize context to JSON
        let ctx_json = serde_json::json!({
            "response": ctx.response,
            "model": ctx.model,
            "session_id": ctx.session_id,
            "duration_ms": ctx.duration_ms,
            "token_count": ctx.token_count,
        });
        let json_str = serde_json::to_string(&ctx_json)?;

        // Call Lua function, ignore return value
        let _: Value = func.call(json_str)?;

        Ok(())
    }
}

impl Default for LlmHookChain {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::events::llm_hook_context::{PostLlmContext, PreLlmContext};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;

    struct PassthroughHook;

    #[async_trait]
    impl LlmHook for PassthroughHook {
        async fn on_pre_llm(&self, ctx: PreLlmContext) -> anyhow::Result<PreLlmResult> {
            Ok(PreLlmResult::Continue(ctx))
        }

        async fn on_post_llm(&self, _ctx: &PostLlmContext) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct ModifyPromptHook {
        suffix: String,
    }

    #[async_trait]
    impl LlmHook for ModifyPromptHook {
        async fn on_pre_llm(&self, mut ctx: PreLlmContext) -> anyhow::Result<PreLlmResult> {
            ctx.prompt.push_str(&self.suffix);
            Ok(PreLlmResult::Continue(ctx))
        }

        async fn on_post_llm(&self, _ctx: &PostLlmContext) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct CancelHook {
        reason: String,
    }

    #[async_trait]
    impl LlmHook for CancelHook {
        async fn on_pre_llm(&self, _ctx: PreLlmContext) -> anyhow::Result<PreLlmResult> {
            Ok(PreLlmResult::Cancel(self.reason.clone()))
        }

        async fn on_post_llm(&self, _ctx: &PostLlmContext) -> anyhow::Result<()> {
            Ok(())
        }
    }

    struct CountingHook {
        pre_count: Arc<AtomicUsize>,
        post_count: Arc<AtomicUsize>,
    }

    #[async_trait]
    impl LlmHook for CountingHook {
        async fn on_pre_llm(&self, ctx: PreLlmContext) -> anyhow::Result<PreLlmResult> {
            self.pre_count.fetch_add(1, Ordering::SeqCst);
            Ok(PreLlmResult::Continue(ctx))
        }

        async fn on_post_llm(&self, _ctx: &PostLlmContext) -> anyhow::Result<()> {
            self.post_count.fetch_add(1, Ordering::SeqCst);
            Ok(())
        }
    }

    struct ErrorHook;

    #[async_trait]
    impl LlmHook for ErrorHook {
        async fn on_pre_llm(&self, _ctx: PreLlmContext) -> anyhow::Result<PreLlmResult> {
            anyhow::bail!("hook failed")
        }

        async fn on_post_llm(&self, _ctx: &PostLlmContext) -> anyhow::Result<()> {
            anyhow::bail!("post hook failed")
        }
    }

    fn make_pre_ctx() -> PreLlmContext {
        PreLlmContext {
            prompt: "hello".into(),
            model: "test-model".into(),
            system_prompt: None,
            context_messages: vec![],
            session_id: "test-session".into(),
        }
    }

    fn make_post_ctx() -> PostLlmContext {
        PostLlmContext {
            response: "world".into(),
            model: "test-model".into(),
            session_id: "test-session".into(),
            duration_ms: 100,
            token_count: Some(10),
        }
    }

    #[tokio::test]
    async fn empty_chain_returns_continue() {
        let chain = LlmHookChain::new();
        let result = chain.run_pre_llm(make_pre_ctx()).await;
        assert!(matches!(result, PreLlmResult::Continue(_)));
    }

    #[tokio::test]
    async fn passthrough_hook_preserves_context() {
        let mut chain = LlmHookChain::new();
        chain.add_hook(Box::new(PassthroughHook));

        let result = chain.run_pre_llm(make_pre_ctx()).await;
        match result {
            PreLlmResult::Continue(ctx) => assert_eq!(ctx.prompt, "hello"),
            PreLlmResult::Cancel(_) => panic!("expected Continue"),
        }
    }

    #[tokio::test]
    async fn hooks_modify_context_sequentially() {
        let mut chain = LlmHookChain::new();
        chain.add_hook(Box::new(ModifyPromptHook {
            suffix: " world".into(),
        }));
        chain.add_hook(Box::new(ModifyPromptHook {
            suffix: "!".into(),
        }));

        let result = chain.run_pre_llm(make_pre_ctx()).await;
        match result {
            PreLlmResult::Continue(ctx) => assert_eq!(ctx.prompt, "hello world!"),
            PreLlmResult::Cancel(_) => panic!("expected Continue"),
        }
    }

    #[tokio::test]
    async fn cancel_short_circuits_chain() {
        let pre_count = Arc::new(AtomicUsize::new(0));
        let post_count = Arc::new(AtomicUsize::new(0));

        let mut chain = LlmHookChain::new();
        chain.add_hook(Box::new(CountingHook {
            pre_count: pre_count.clone(),
            post_count: post_count.clone(),
        }));
        chain.add_hook(Box::new(CancelHook {
            reason: "blocked".into(),
        }));
        chain.add_hook(Box::new(CountingHook {
            pre_count: pre_count.clone(),
            post_count: post_count.clone(),
        }));

        let result = chain.run_pre_llm(make_pre_ctx()).await;
        assert!(matches!(result, PreLlmResult::Cancel(r) if r == "blocked"));

        assert_eq!(pre_count.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn pre_llm_error_returns_continue_fail_open() {
        let mut chain = LlmHookChain::new();
        chain.add_hook(Box::new(ErrorHook));

        let result = chain.run_pre_llm(make_pre_ctx()).await;

        assert!(matches!(result, PreLlmResult::Continue(_)));
    }

    #[tokio::test]
    async fn post_llm_runs_all_hooks_despite_errors() {
        let count = Arc::new(AtomicUsize::new(0));

        let mut chain = LlmHookChain::new();
        chain.add_hook(Box::new(CountingHook {
            pre_count: Arc::new(AtomicUsize::new(0)),
            post_count: count.clone(),
        }));
        chain.add_hook(Box::new(ErrorHook));
        chain.add_hook(Box::new(CountingHook {
            pre_count: Arc::new(AtomicUsize::new(0)),
            post_count: count.clone(),
        }));

        chain.run_post_llm(&make_post_ctx()).await;

        assert_eq!(count.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn empty_chain_post_llm_succeeds() {
        let chain = LlmHookChain::new();
        chain.run_post_llm(&make_post_ctx()).await;

    }
}
