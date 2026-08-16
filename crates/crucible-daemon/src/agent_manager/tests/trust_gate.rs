//! The attach-time trust invariant, checked at the two places a live
//! session's provider can change: `configure_agent` and `switch_model`.
//!
//! `search_across_kilns` does no trust filtering on the primary kiln and
//! `tools/search.rs` passes `provider_trust: None` outright, both on the
//! strength of "the kiln passed the trust gate when it was attached". These
//! tests are what keeps that strength honest.
use crate::test_support::temp_session_manager;

use super::*;
use crucible_core::config::{LlmConfig, LlmProviderConfig, TrustLevel};

/// Two providers straddling a confidential kiln: `local` clears it, `cloud`
/// does not.
fn straddling_providers() -> LlmConfig {
    let provider = |provider_type: BackendType, trust: TrustLevel| LlmProviderConfig {
        provider_type,
        endpoint: None,
        default_model: None,
        temperature: None,
        max_tokens: None,
        timeout_secs: None,
        api_key: None,
        available_models: None,
        trust_level: Some(trust),
        name: None,
    };
    LlmConfig {
        default: None,
        providers: HashMap::from([
            (
                "local".to_string(),
                provider(BackendType::Ollama, TrustLevel::Local),
            ),
            (
                "cloud".to_string(),
                provider(BackendType::OpenAI, TrustLevel::Cloud),
            ),
        ]),
        models: Default::default(),
    }
}

/// `<tmp>/ws` is a project declaring `<tmp>/ws/notes` confidential; the kiln
/// itself carries no config, so this also exercises the walk-up that
/// `find_workspace_and_resolve_classification` does.
fn confidential_kiln(tmp: &std::path::Path) -> PathBuf {
    let workspace = tmp.join("ws");
    let kiln = workspace.join("notes");
    std::fs::create_dir_all(&kiln).unwrap();
    let crucible = workspace.join(".crucible");
    std::fs::create_dir_all(&crucible).unwrap();
    std::fs::write(
        crucible.join("project.toml"),
        "[[kilns]]\npath = \"./notes\"\ndata_classification = \"confidential\"\n",
    )
    .unwrap();
    kiln
}

fn agent_on(provider_key: &str, provider: BackendType) -> SessionAgent {
    SessionAgent {
        provider_key: Some(provider_key.to_string()),
        provider,
        ..test_agent()
    }
}

async fn session_on(
    session_manager: &SessionManager,
    kiln: PathBuf,
    connected: Vec<PathBuf>,
) -> crucible_core::session::Session {
    session_manager
        .create_session(
            SessionType::Chat,
            std::iter::once(kiln).chain(connected).collect(),
            None,
            None,
        )
        .await
        .unwrap()
}

/// The two-step bypass: create on a provider the kiln clears, then reconfigure
/// onto one it does not. Create-time gating alone never sees the second step.
#[tokio::test]
async fn configure_agent_cannot_raise_provider_trust_past_an_attached_kiln() {
    let tmp = TempDir::new().unwrap();
    let kiln = confidential_kiln(tmp.path());
    let session_manager = temp_session_manager();
    let session = session_on(&session_manager, kiln, vec![]).await;
    let am =
        create_test_agent_manager_with_llm_config(session_manager.clone(), straddling_providers());

    // Step 1 is legitimate and must stay legitimate: a Local provider clears a
    // Confidential kiln, which is exactly what create-time gating allows.
    am.configure_agent(&session.id, agent_on("local", BackendType::Ollama))
        .await
        .expect("a local provider clears a confidential kiln");

    let err = am
        .configure_agent(&session.id, agent_on("cloud", BackendType::OpenAI))
        .await
        .expect_err("step 2 must not be the cheaper door into the kiln");
    assert!(matches!(err, AgentError::InvalidConfig(_)), "got: {err:?}");
    assert!(
        err.to_string()
            .contains("insufficient for the attached kiln"),
        "got: {err}"
    );

    let stored = session_manager
        .get_session(&session.id)
        .unwrap()
        .agent
        .expect("step 1's agent survives");
    assert_eq!(
        stored.provider_key.as_deref(),
        Some("local"),
        "a refused configure must not have persisted"
    );
}

/// Connected kilns count too — they are searched alongside the primary one, so
/// gating only `session.kiln` would leak every kiln attached after create.
#[tokio::test]
async fn configure_agent_checks_connected_kilns_not_only_the_primary() {
    let tmp = TempDir::new().unwrap();
    let confidential = confidential_kiln(tmp.path());
    let public = tmp.path().join("public");
    std::fs::create_dir_all(&public).unwrap();

    let session_manager = temp_session_manager();
    let session = session_on(&session_manager, public, vec![confidential]).await;
    let am =
        create_test_agent_manager_with_llm_config(session_manager.clone(), straddling_providers());

    let err = am
        .configure_agent(&session.id, agent_on("cloud", BackendType::OpenAI))
        .await
        .expect_err("a confidential connected kiln must refuse a cloud provider");
    assert!(
        err.to_string()
            .contains("insufficient for the attached kiln"),
        "got: {err}"
    );

    am.configure_agent(&session.id, agent_on("local", BackendType::Ollama))
        .await
        .expect("a local provider clears it");
}

/// The other door onto the same invariant. `switch_model` has always been
/// gated; this pins it now that the gate is shared with `configure_agent`.
#[tokio::test]
async fn switch_model_cannot_raise_provider_trust_past_an_attached_kiln() {
    let tmp = TempDir::new().unwrap();
    let kiln = confidential_kiln(tmp.path());
    let session_manager = temp_session_manager();
    let session = session_on(&session_manager, kiln, vec![]).await;
    let am =
        create_test_agent_manager_with_llm_config(session_manager.clone(), straddling_providers());

    am.configure_agent(&session.id, agent_on("local", BackendType::Ollama))
        .await
        .unwrap();

    let err = am
        .switch_model(&session.id, "cloud/gpt-4o", None)
        .await
        .expect_err("switching onto a cloud provider must be refused");
    assert!(
        err.to_string()
            .contains("insufficient for the attached kiln"),
        "got: {err}"
    );

    am.switch_model(&session.id, "local/llama3.2", None)
        .await
        .expect("staying on a local provider is fine");
}

/// Trust follows the provider, not the presence of a name.
///
/// `agent_name` on an internal agent is the deprecated agent-card alias (and
/// what Discord sets today). Reading it as "this is ACP" pinned such a session
/// to `Cloud`, which is strictly below `Local`, so a local Ollama session was
/// refused on a confidential kiln it plainly clears.
#[tokio::test]
async fn a_named_internal_agent_is_trusted_at_its_providers_level() {
    let tmp = TempDir::new().unwrap();
    let kiln = confidential_kiln(tmp.path());
    let session_manager = temp_session_manager();
    let session = session_on(&session_manager, kiln, vec![]).await;
    let am =
        create_test_agent_manager_with_llm_config(session_manager.clone(), straddling_providers());

    let named = SessionAgent {
        agent_name: Some("researcher".to_string()),
        ..agent_on("local", BackendType::Ollama)
    };
    am.configure_agent(&session.id, named)
        .await
        .expect("an internal agent with a name is still an internal agent");
}

/// The direction that matters for security: `agent_type == "acp"` is Cloud
/// regardless of what `provider_key` claims, because the external process
/// picks its own model.
#[tokio::test]
async fn an_acp_agent_is_cloud_whatever_its_provider_key_says() {
    let tmp = TempDir::new().unwrap();
    let kiln = confidential_kiln(tmp.path());
    let session_manager = temp_session_manager();
    let session = session_on(&session_manager, kiln, vec![]).await;
    let am =
        create_test_agent_manager_with_llm_config(session_manager.clone(), straddling_providers());

    let acp = SessionAgent {
        agent_type: "acp".to_string(),
        agent_name: Some("claude".to_string()),
        ..agent_on("local", BackendType::Ollama)
    };
    let err = am
        .configure_agent(&session.id, acp)
        .await
        .expect_err("an ACP agent must not borrow a local provider's trust");
    assert!(
        err.to_string()
            .contains("insufficient for the attached kiln"),
        "got: {err}"
    );
}
