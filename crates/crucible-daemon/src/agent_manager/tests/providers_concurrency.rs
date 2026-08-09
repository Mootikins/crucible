//! Provider probing must not serialize.
//!
//! `list_providers` is on the `session.create` setup path, and each probe waits
//! up to `LIST_MODELS_TIMEOUT` (10s, `provider/model_listing.rs`). Probed one
//! after another, a user with three providers and one dead endpoint waits up to
//! thirty seconds for a session to finish setting up — and the
//! `providers_listed` setup event, which the TUI uses to warn about a
//! provider-less install, arrives that late.

use crate::agent_manager::{AgentManager, AgentManagerParams};
use crate::background_manager::BackgroundJobManager;
use crate::kiln_manager::KilnManager;
use crate::session_manager::SessionManager;
use crate::tools::WorkspaceTools;
use crucible_core::config::{BackendType, LlmConfig, LlmProviderConfig};
use std::sync::Arc;
use std::time::{Duration, Instant};
use tokio::sync::broadcast;

/// A listener that accepts and then never answers, so a probe against it waits
/// for `delay` and gives up. Returns the endpoint to point a provider at.
///
/// A real socket rather than an unroutable address: connect-refused would come
/// back instantly and prove nothing, and a blackhole IP makes the test depend
/// on the host's routing table.
async fn stalling_endpoint(delay: Duration) -> String {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        loop {
            match listener.accept().await {
                Ok((stream, _)) => {
                    tokio::spawn(async move {
                        tokio::time::sleep(delay).await;
                        drop(stream);
                    });
                }
                Err(_) => break,
            }
        }
    });
    format!("http://{addr}")
}

/// One declared provider per chat-capable backend, each with `available_models`
/// so it never dials.
///
/// This is the test's hermeticity, and it is load-bearing: `iter_chat_providers`
/// supplements the configured list with `discover_env_providers`, which reads
/// `OLLAMA_HOST` and the per-backend API-key variables straight from the
/// process environment. An in-process test inherits the developer's shell, so
/// without this the suite silently gains whatever providers happen to be
/// exported — the first draft of this test found four providers where it
/// declared three, and would have dialled a real endpoint. Declaring a backend
/// puts it in `seen_types`, and env discovery skips backends already seen.
fn claim_every_backend() -> std::collections::HashMap<String, LlmProviderConfig> {
    let backends = [
        BackendType::Ollama,
        BackendType::OpenAI,
        BackendType::Anthropic,
        BackendType::Cohere,
        BackendType::VertexAI,
        BackendType::FastEmbed,
        BackendType::Burn,
        BackendType::GitHubCopilot,
        BackendType::OpenRouter,
        BackendType::ZAI,
        BackendType::Custom,
        BackendType::Mock,
    ];
    backends
        .into_iter()
        .filter(|b| b.supports_chat())
        .map(|b| {
            (
                format!("declared-{}", b.as_str()),
                LlmProviderConfig::builder(b)
                    .available_models(vec!["pinned".to_string()])
                    .build(),
            )
        })
        .collect()
}

fn manager_with(providers: std::collections::HashMap<String, LlmProviderConfig>) -> AgentManager {
    let (event_tx, _) = broadcast::channel(16);
    AgentManager::new(AgentManagerParams {
        kiln_manager: Arc::new(KilnManager::new()),
        session_manager: Arc::new(SessionManager::new()),
        background_manager: Arc::new(BackgroundJobManager::new(event_tx)),
        mcp_gateway: None,
        llm_config: Some(LlmConfig {
            default: None,
            providers,
            models: Default::default(),
        }),
        acp_config: None,
        context_config: None,
        permission_config: None,
        plugin_loader: None,
        workspace_tools: Arc::new(WorkspaceTools::new(std::path::PathBuf::from("/tmp"))),
    })
}

/// Three unresponsive providers must cost roughly one probe, not three.
///
/// Deliberately generous: the assertion is "clearly not the sum", not a precise
/// timing. Serial would be >=1500ms; concurrent lands near 500ms.
#[tokio::test]
async fn unresponsive_providers_are_probed_concurrently() {
    let delay = Duration::from_millis(500);
    let mut providers = claim_every_backend();
    for i in 0..3 {
        // No `available_models`: that short-circuits discovery before any dial,
        // which is exactly the path this test must not take.
        providers.insert(
            format!("stalled{i}"),
            LlmProviderConfig::builder(BackendType::OpenAI)
                .endpoint(stalling_endpoint(delay).await)
                .build(),
        );
    }
    let declared = providers.len();

    let am = manager_with(providers);

    let started = Instant::now();
    let listed = am.list_providers(None).await;
    let elapsed = started.elapsed();

    assert_eq!(listed.len(), declared, "every provider is still reported");
    assert!(
        elapsed < delay * 2,
        "three 500ms probes took {elapsed:?} — they are being awaited one at a time"
    );
}
