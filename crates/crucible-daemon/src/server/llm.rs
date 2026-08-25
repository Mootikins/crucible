//! `llm.*` — the provider selection the daemon records and serves.
//!
//! One method, for the same reason `kiln.register` exists: `cru init` and the
//! setup wizard used to edit the user's config file from the CLI, which put
//! storage in a layer that must not have any and buried a machine-written
//! answer inside a hand-written file.

use std::sync::Arc;

use crucible_core::config::{BackendType, LlmProviderConfig};
use crucible_core::protocol::rpc::{Request, Response, INVALID_PARAMS};
use tracing::info;

use crate::llm_state::SelectionOutcome;
use crate::rpc_helpers::typed_params;

/// `llm.register_provider`: record which provider and model to use.
///
/// The write always lands. Whether the RUNNING daemon honours it follows the
/// rule the kiln registry already holds — **an addition is live, a re-point
/// waits for the next bind** — and the reply says which happened.
///
/// Saying so is not politeness. The failure this whole path exists to remove is
/// a user who picks Anthropic, is told it worked, and then chats against the
/// old provider. A silent defer is that same failure wearing a different hat,
/// so the reply names what is still serving in the meantime.
pub(crate) async fn handle_llm_register_provider(
    req: Request,
    state: &Arc<crate::llm_state::LlmStateStore>,
    live: &crate::llm_state::LiveLlmConfig,
) -> Response {
    let params = match typed_params::<crate::rpc_client::LlmRegisterProviderRequest>(&req) {
        Ok(p) => p,
        Err(response) => return *response,
    };

    // Validated before anything is written: an unknown type must fail here
    // rather than produce a state file the daemon then refuses to load.
    let provider_type: BackendType = match params.provider.parse() {
        Ok(t) => t,
        Err(_) => {
            return Response::error(
                req.id,
                INVALID_PARAMS,
                format!(
                    "unknown provider type '{}'. Valid types: {}.",
                    params.provider,
                    BackendType::all()
                        .iter()
                        .map(|t| t.as_str())
                        .collect::<Vec<_>>()
                        .join(", ")
                ),
            )
        }
    };

    // What the daemon serves right now, read BEFORE the write so a deferred
    // reply can name it. After the write it would be the same value either way
    // and the message would be useless in the case that needs it.
    let serving_now = live.get().and_then(|c| c.default.clone());

    let outcome = match state.register_provider(
        &params.provider,
        provider_type,
        &params.model,
        params.make_default,
    ) {
        Ok(outcome) => outcome,
        Err(e) => return Response::error(req.id, INVALID_PARAMS, e.to_string()),
    };

    // The store decided; the live table only executes that decision. Both use
    // the same additive rule, so `add_provider` returning false where the store
    // said Additive would mean the two had drifted — asserted rather than
    // papered over, because a silent disagreement here is a lie in the reply.
    let applied = if outcome == SelectionOutcome::Additive {
        let mut entry = LlmProviderConfig::builder(provider_type).build();
        entry.default_model = Some(params.model.clone());
        live.add_provider(&params.provider, entry, params.make_default)
    } else {
        false
    };

    info!(
        provider = params.provider,
        model = params.model,
        outcome = outcome.as_str(),
        applied,
        "LLM provider selection recorded"
    );

    Response::success(
        req.id,
        serde_json::json!({
            "status": "ok",
            "provider": params.provider,
            "model": params.model,
            "state_file": state.path().to_string_lossy(),
            "outcome": outcome.as_str(),
            // The single field a client needs to decide what to print.
            "live": applied,
            // What the daemon keeps using until the next start, when the
            // answer is deferred. `null` when it was applied, or when nothing
            // was serving.
            "still_serving": if applied { None } else { serving_now },
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::LlmConfig;
    use tempfile::TempDir;

    fn request(provider: &str, model: &str, make_default: bool) -> Request {
        serde_json::from_value(serde_json::json!({
            "jsonrpc": "2.0",
            "id": 1,
            "method": "llm.register_provider",
            "params": {
                "provider": provider,
                "model": model,
                "make_default": make_default,
            },
        }))
        .unwrap()
    }

    fn store(dir: &TempDir) -> Arc<crate::llm_state::LlmStateStore> {
        Arc::new(crate::llm_state::LlmStateStore::new(dir.path()))
    }

    /// First-run `cru init`: nothing configured, nothing bound. The selection
    /// lands in the file AND in the running daemon, so the very next
    /// `session.create` uses it. This is the flow the whole path exists for.
    #[tokio::test]
    async fn a_first_selection_is_recorded_and_served_without_a_restart() {
        let tmp = TempDir::new().unwrap();
        let state = store(&tmp);
        let live = crate::llm_state::LiveLlmConfig::new(None);

        let resp = handle_llm_register_provider(
            request("anthropic", "claude-sonnet", true),
            &state,
            &live,
        )
        .await;
        let data = resp.result.expect("the selection is recorded");

        assert_eq!(data["outcome"], "additive");
        assert_eq!(data["live"], true);
        assert_eq!(data["still_serving"], serde_json::Value::Null);

        let table = live.get().expect("the running table now has a provider");
        assert_eq!(table.default.as_deref(), Some("anthropic"));
        assert_eq!(
            table.providers["anthropic"].default_model.as_deref(),
            Some("claude-sonnet")
        );
    }

    /// Re-running `cru init` to SWITCH providers. The write lands, the running
    /// daemon does not take it, and the reply NAMES what is still serving.
    ///
    /// The naming is the requirement, not a nicety: "recorded, and you are
    /// still on ollama" is actionable, "recorded" alone leaves the user
    /// believing they are chatting against Anthropic. That belief is the
    /// original bug.
    #[tokio::test]
    async fn a_deferred_selection_says_what_is_still_serving() {
        let tmp = TempDir::new().unwrap();
        let state = store(&tmp);
        state
            .register_provider(
                "ollama",
                crucible_core::config::BackendType::Ollama,
                "llama3.2",
                true,
            )
            .unwrap();

        let mut bound = LlmConfig::default();
        bound.providers.insert(
            "ollama".to_string(),
            LlmProviderConfig::builder(crucible_core::config::BackendType::Ollama).build(),
        );
        bound.default = Some("ollama".to_string());
        let live = crate::llm_state::LiveLlmConfig::new(Some(bound));

        let resp = handle_llm_register_provider(
            request("anthropic", "claude-sonnet", true),
            &state,
            &live,
        )
        .await;
        let data = resp.result.expect("the selection is still recorded");

        assert_eq!(data["outcome"], "deferred");
        assert_eq!(data["live"], false);
        assert_eq!(
            data["still_serving"], "ollama",
            "a deferral must name what the daemon keeps using: {data}"
        );

        // The file has it; the running table does not.
        assert!(state.read().unwrap().providers.contains_key("anthropic"));
        let table = live.get().unwrap();
        assert!(
            !table.providers.contains_key("anthropic"),
            "a re-point must not land in a table live sessions resolve against"
        );
        assert_eq!(table.default.as_deref(), Some("ollama"));
    }

    /// A second provider that does not claim the default is additive, so it is
    /// available immediately without changing what any session resolved.
    #[tokio::test]
    async fn a_second_provider_that_leaves_the_default_alone_lands_live() {
        let tmp = TempDir::new().unwrap();
        let state = store(&tmp);
        state
            .register_provider(
                "ollama",
                crucible_core::config::BackendType::Ollama,
                "llama3.2",
                true,
            )
            .unwrap();

        let mut bound = LlmConfig::default();
        bound.providers.insert(
            "ollama".to_string(),
            LlmProviderConfig::builder(crucible_core::config::BackendType::Ollama).build(),
        );
        bound.default = Some("ollama".to_string());
        let live = crate::llm_state::LiveLlmConfig::new(Some(bound));

        let resp = handle_llm_register_provider(
            request("anthropic", "claude-sonnet", false),
            &state,
            &live,
        )
        .await;
        let data = resp.result.expect("recorded");

        assert_eq!(data["outcome"], "additive");
        assert_eq!(data["live"], true);
        let table = live.get().unwrap();
        assert!(table.providers.contains_key("anthropic"));
        assert_eq!(
            table.default.as_deref(),
            Some("ollama"),
            "an additive add must not move the default out from under a session"
        );
    }

    /// An unknown provider type fails before anything is written, rather than
    /// producing a state file the daemon then refuses to load.
    #[tokio::test]
    async fn an_unknown_provider_type_is_refused_and_writes_nothing() {
        let tmp = TempDir::new().unwrap();
        let state = store(&tmp);
        let live = crate::llm_state::LiveLlmConfig::new(None);

        let resp =
            handle_llm_register_provider(request("nonesuch", "m", true), &state, &live).await;

        let err = resp.error.expect("an unknown type must be refused");
        assert!(err.message.contains("nonesuch"), "{}", err.message);
        assert!(
            err.message.contains("ollama"),
            "name the valid set: {}",
            err.message
        );
        assert!(!state.path().exists(), "nothing is written");
        assert!(live.get().is_none(), "the running table is untouched");
    }
}
