//! Local-only provider detection for `cru init`.
//!
//! This module exists because `cru init` runs before the daemon is started,
//! so it cannot use the `providers.list` RPC. It performs env-var and
//! credential-store checks; the one piece of network traffic is
//! [`detect_providers_probed`]'s TCP dial to the Ollama endpoint.
//!
//! For runtime provider discovery (after daemon is running), use
//! `DaemonClient::list_providers()` instead. Model context-length fetches
//! also run daemon-side — see
//! `crucible_daemon::agent_manager::context_length::fetch_model_context_length`.

use crucible_core::config::credentials::{discover_credentials, CredentialSource, SecretsFile};
use crucible_core::config::{
    ollama_endpoint_from_env, BackendType, ChatConfig, DEFAULT_OLLAMA_ENDPOINT,
};
use crucible_core::types::ProviderInfo;

/// A provider `cru init` found, with where its credential came from.
///
/// The daemon describes a provider with [`ProviderInfo`]. This is the same
/// record, seen before the daemon runs, plus the credential source that
/// ranks the entries. `Deref` gives callers the `ProviderInfo` fields.
#[derive(Debug, Clone)]
pub struct DetectedProvider {
    pub info: ProviderInfo,
    pub source: Option<CredentialSource>,
}

impl std::ops::Deref for DetectedProvider {
    type Target = ProviderInfo;

    fn deref(&self) -> &ProviderInfo {
        &self.info
    }
}

impl DetectedProvider {
    /// Why the provider is listed, for a person to read.
    pub fn reason(&self) -> &str {
        self.info.reason.as_deref().unwrap_or_default()
    }
}

/// Detect available providers from config, environment, and the credential
/// store — no network traffic.
///
/// Every keyed backend with a credential is listed, plus an unconditional
/// Ollama entry whose `available: true` is an assumption ("we know where it
/// would be"), not a probe result. Credential-backed providers rank first.
pub fn detect_providers(config: &ChatConfig) -> Vec<DetectedProvider> {
    detect_providers_inner(config, false, &SecretsFile::new())
}

/// Like [`detect_providers`], but verifies the Ollama endpoint actually
/// answers (a TCP dial capped at ~300ms — the module's one exception to
/// "no network"). `cru init` and the wizard use this: writing an unreachable
/// provider into a fresh config is the exact bug being prevented. Per-launch
/// callers keep the dial-free variant.
pub fn detect_providers_probed(config: &ChatConfig) -> Vec<DetectedProvider> {
    detect_providers_inner(config, true, &SecretsFile::new())
}

fn detect_providers_inner(
    config: &ChatConfig,
    probe: bool,
    store: &SecretsFile,
) -> Vec<DetectedProvider> {
    // The scan is the daemon's scan. Ollama gets its own entry below, so the
    // scan's OLLAMA_HOST hit is dropped here.
    let mut providers: Vec<DetectedProvider> = discover_credentials(Some(store))
        .into_iter()
        .filter(|found| found.backend != BackendType::Ollama)
        .map(|found| {
            let backend = found.backend;
            DetectedProvider {
                info: ProviderInfo {
                    name: backend.label().to_string(),
                    provider_type: backend.as_str().to_string(),
                    available: true,
                    default_model: config
                        .model
                        .clone()
                        .or_else(|| backend.default_chat_model().map(str::to_string)),
                    models: Vec::new(),
                    endpoint: backend.default_endpoint().map(str::to_string),
                    reason: Some(found.reason),
                    is_local: backend.is_local(),
                },
                source: Some(found.source),
            }
        })
        .collect();

    // Ollama is always offered: it is the no-credential path. `available` is
    // an assumption unless the caller asked for a probe. The shared helper
    // treats `OLLAMA_HOST=""` as unset, so the CLI and the daemon agree.
    let env_endpoint = ollama_endpoint_from_env();
    let ollama_host_set = env_endpoint.is_some();
    let endpoint = env_endpoint.unwrap_or_else(|| {
        config
            .endpoint
            .clone()
            .unwrap_or_else(|| DEFAULT_OLLAMA_ENDPOINT.to_string())
    });
    let mut reason = if ollama_host_set {
        format!("OLLAMA_HOST={}", endpoint)
    } else if config.endpoint.is_some() {
        format!("config endpoint={}", endpoint)
    } else {
        "config provider=ollama".to_string()
    };
    let available = if probe {
        let answers = endpoint_answers(&endpoint, std::time::Duration::from_millis(300));
        if !answers {
            reason = format!("{reason} (not answering at {endpoint})");
        }
        answers
    } else {
        true
    };
    providers.push(DetectedProvider {
        info: ProviderInfo {
            name: "Ollama (Local)".to_string(),
            provider_type: "ollama".to_string(),
            available,
            default_model: config.model.clone(),
            models: Vec::new(),
            endpoint: Some(endpoint),
            reason: Some(reason),
            is_local: BackendType::Ollama.is_local(),
        },
        source: None,
    });

    // Rank credential-backed providers ahead of the assumed local default,
    // and reachable ones ahead of dead ones. `cru init -y` picks from the
    // front, so without this a user whose only credential is
    // ANTHROPIC_API_KEY got an Ollama kiln. Stable sort preserves the
    // `BackendType::all()` order within each group.
    providers.sort_by_key(|p| (p.source.is_none(), !p.available));

    providers
}

/// Whether anything is listening at an `http(s)://host:port` endpoint.
/// A TCP dial, not an HTTP request — cheap enough for interactive setup.
///
/// The timeout bounds the *whole* probe, including DNS: `to_socket_addrs`
/// resolves synchronously with no timeout of its own, and a hostname that
/// doesn't resolve (an `OLLAMA_HOST` pointing at an off-VPN box) would
/// otherwise freeze `cru init` for the resolver's full multi-second budget.
/// The worker thread is deliberately detached — it may outlive the wait,
/// but the caller never blocks past `timeout`.
fn endpoint_answers(endpoint: &str, timeout: std::time::Duration) -> bool {
    let (tx, rx) = std::sync::mpsc::channel();
    let endpoint = endpoint.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(dial_endpoint(&endpoint, timeout));
    });
    rx.recv_timeout(timeout).unwrap_or(false)
}

fn dial_endpoint(endpoint: &str, timeout: std::time::Duration) -> bool {
    let hostport = endpoint
        .strip_prefix("http://")
        .or_else(|| endpoint.strip_prefix("https://"))
        .unwrap_or(endpoint);
    let hostport = hostport.split('/').next().unwrap_or(hostport);
    let addr = if hostport.contains(':') {
        hostport.to_string()
    } else {
        format!("{hostport}:11434")
    };

    use std::net::ToSocketAddrs;
    let Ok(addrs) = addr.to_socket_addrs() else {
        return false;
    };
    addrs
        .into_iter()
        .any(|a| std::net::TcpStream::connect_timeout(&a, timeout).is_ok())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::test_support::EnvVarGuard;
    use serial_test::serial;

    /// Detection against an empty, isolated credential store. The default
    /// store is the developer's real secrets.toml — using it makes these
    /// tests pass on CI and fail on any box with stored credentials.
    fn detect_isolated(config: &ChatConfig, probe: bool) -> Vec<DetectedProvider> {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = SecretsFile::with_path(tmp.path().join("secrets.toml"));
        detect_providers_inner(config, probe, &store)
    }

    /// `cru init -y` selects `providers[0]`, and the Ollama entry is pushed
    /// first with `available: true` and no probe behind it. A user whose only
    /// credential is an Anthropic key must not be handed an Ollama kiln.
    #[test]
    #[serial]
    fn a_credentialled_provider_outranks_the_unprobed_local_default() {
        let _key = EnvVarGuard::set("ANTHROPIC_API_KEY", "sk-ant-test".to_string());
        let detected = detect_isolated(&ChatConfig::default(), false);

        let anthropic = detected
            .iter()
            .position(|p| p.provider_type == "anthropic")
            .expect("an Anthropic key in the environment must be detected");
        let ollama = detected
            .iter()
            .position(|p| p.provider_type == "ollama")
            .expect("Ollama is always listed");

        assert!(
            anthropic < ollama,
            "a provider with a real credential must rank above an unprobed local default: {detected:#?}"
        );
    }

    /// Ranks first only when nothing else has a credential — this reads the
    /// process environment, so every keyed backend's env var must be cleared
    /// or the test passes on CI and fails on a developer box that exports one
    /// (GLM_AUTH_TOKEN did exactly that when Z.AI detection came alive).
    #[test]
    #[serial]
    fn test_detect_ollama_from_default_config() {
        let _openai = EnvVarGuard::remove("OPENAI_API_KEY");
        let _anthropic = EnvVarGuard::remove("ANTHROPIC_API_KEY");
        let _openrouter = EnvVarGuard::remove("OPENROUTER_API_KEY");
        let _zai = EnvVarGuard::remove("GLM_AUTH_TOKEN");
        let config = ChatConfig::default();
        let detected = detect_isolated(&config, false);
        assert!(!detected.is_empty());
        assert_eq!(detected[0].provider_type, "ollama");
        assert!(detected[0].reason().contains("config provider=ollama"));
    }

    #[test]
    #[serial]
    fn test_detect_ollama_from_env() {
        let _guard = EnvVarGuard::set("OLLAMA_HOST", "http://myhost:11434".to_string());
        let config = ChatConfig::default();
        let detected = detect_isolated(&config, false);
        assert!(!detected.is_empty());
        let ollama = detected
            .iter()
            .find(|p| p.provider_type == "ollama")
            .unwrap();
        assert!(ollama.reason().contains("OLLAMA_HOST"));
    }

    #[test]
    #[serial]
    fn test_detect_openai_from_config_with_key() {
        let _guard = EnvVarGuard::set("OPENAI_API_KEY", "sk-test".to_string());
        let config = ChatConfig::default();
        let detected = detect_isolated(&config, false);
        assert!(detected.iter().any(|p| p.provider_type == "openai"));
    }

    #[test]
    #[serial]
    fn test_detect_openai_from_config_without_key_is_empty() {
        let _guard1 = EnvVarGuard::remove("OPENAI_API_KEY");
        let _guard2 = EnvVarGuard::remove("ANTHROPIC_API_KEY");
        let config = ChatConfig::default();
        let detected = detect_isolated(&config, false);
        // No API key = no provider detected for cloud providers
        assert!(!detected.iter().any(|p| p.provider_type == "openai"));
    }

    #[test]
    #[serial]
    fn test_detect_extra_providers_from_env() {
        let _guard = EnvVarGuard::set("ANTHROPIC_API_KEY", "sk-ant-test".to_string());
        let config = ChatConfig::default(); // ollama config
        let detected = detect_isolated(&config, false);
        // Should have ollama from config + anthropic from env
        assert!(detected.iter().any(|p| p.provider_type == "ollama"));
        assert!(detected.iter().any(|p| p.provider_type == "anthropic"));
    }

    /// Credential lookup against an empty, isolated store, so only the
    /// environment can satisfy it.
    fn has_key_isolated(provider: &str) -> bool {
        let tmp = tempfile::TempDir::new().unwrap();
        let store = SecretsFile::with_path(tmp.path().join("secrets.toml"));
        let Ok(backend) = provider.parse::<BackendType>() else {
            return false;
        };
        discover_credentials(Some(&store))
            .iter()
            .any(|c| c.backend == backend)
    }

    #[test]
    #[serial]
    fn test_has_api_key_openai() {
        let _guard = EnvVarGuard::set("OPENAI_API_KEY", "sk-test".to_string());
        assert!(has_key_isolated("openai"));
    }

    #[test]
    #[serial]
    fn test_has_api_key_anthropic() {
        let _guard = EnvVarGuard::set("ANTHROPIC_API_KEY", "sk-ant-test".to_string());
        assert!(has_key_isolated("anthropic"));
    }

    #[test]
    fn test_has_api_key_unknown_provider() {
        assert!(!has_key_isolated("unknown"));
        assert!(!has_key_isolated("google"));
    }

    #[test]
    #[serial]
    fn test_has_api_key_case_insensitive() {
        let _guard = EnvVarGuard::set("OPENAI_API_KEY", "sk-test".to_string());
        assert!(has_key_isolated("OpenAI"));
        assert!(has_key_isolated("OPENAI"));
        assert!(has_key_isolated("openai"));
    }

    #[test]
    #[serial]
    fn test_has_api_key_missing() {
        let _guard1 = EnvVarGuard::remove("OPENAI_API_KEY");
        let _guard2 = EnvVarGuard::remove("ANTHROPIC_API_KEY");
        assert!(!has_key_isolated("openai"));
        assert!(!has_key_isolated("anthropic"));
    }

    #[test]
    fn test_detected_provider_struct() {
        let provider = DetectedProvider {
            info: ProviderInfo {
                name: "Test Provider".to_string(),
                provider_type: "test".to_string(),
                available: true,
                default_model: Some("test-model".to_string()),
                models: Vec::new(),
                endpoint: None,
                reason: Some("Test reason".to_string()),
                is_local: false,
            },
            source: Some(CredentialSource::EnvVar),
        };

        assert_eq!(provider.name, "Test Provider");
        assert_eq!(provider.provider_type, "test");
        assert!(provider.available);
        assert_eq!(provider.reason(), "Test reason");
        assert_eq!(provider.default_model, Some("test-model".to_string()));
    }

    /// OpenRouter and Z.AI were only reachable through match arms that a
    /// hardcoded `BackendType::Ollama` made dead code — an exported key for
    /// either was silently invisible to `cru init`.
    #[test]
    #[serial]
    fn openrouter_and_zai_keys_are_detected() {
        let _or = EnvVarGuard::set("OPENROUTER_API_KEY", "sk-or-test".to_string());
        let _zai = EnvVarGuard::set("GLM_AUTH_TOKEN", "zai-test".to_string());

        let detected = detect_isolated(&ChatConfig::default(), false);

        assert!(
            detected.iter().any(|p| p.provider_type == "openrouter"),
            "an OpenRouter key must be detected: {detected:#?}"
        );
        assert!(
            detected.iter().any(|p| p.provider_type == "zai"),
            "a Z.AI key must be detected: {detected:#?}"
        );
    }

    #[test]
    fn the_probe_sees_a_listening_socket() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let endpoint = format!("http://{}", listener.local_addr().unwrap());

        assert!(endpoint_answers(
            &endpoint,
            std::time::Duration::from_millis(300)
        ));
    }

    /// DNS has no timeout of its own; the probe's budget must bound the
    /// whole operation or an unresolvable OLLAMA_HOST freezes `cru init`
    /// for the resolver's multi-second retry schedule.
    #[test]
    fn the_probe_gives_up_within_its_budget_even_for_dns() {
        let start = std::time::Instant::now();
        let answered = endpoint_answers(
            "http://nonexistent-host.invalid:11434",
            std::time::Duration::from_millis(300),
        );

        assert!(!answered);
        assert!(
            start.elapsed() < std::time::Duration::from_millis(1500),
            "the probe must not block past its budget, took {:?}",
            start.elapsed()
        );
    }

    /// `OLLAMA_HOST=""` must behave like unset — the daemon's env discovery
    /// filters empty values and detection has to agree, or the empty export
    /// yields the endpoint `http://`.
    #[test]
    #[serial]
    fn an_empty_ollama_host_is_treated_as_unset() {
        let _host = EnvVarGuard::set("OLLAMA_HOST", String::new());

        let detected = detect_isolated(&ChatConfig::default(), false);
        let ollama = detected
            .iter()
            .find(|p| p.provider_type == "ollama")
            .unwrap();

        assert!(
            !ollama.reason().contains("OLLAMA_HOST"),
            "an empty OLLAMA_HOST must not be reported as the source: {}",
            ollama.reason()
        );
    }

    #[test]
    fn the_probe_reports_a_closed_port() {
        // Bind-then-drop guarantees the port was just free.
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };

        assert!(!endpoint_answers(
            &format!("http://127.0.0.1:{port}"),
            std::time::Duration::from_millis(300)
        ));
    }

    /// `cru init` must not write an unreachable Ollama into a fresh config
    /// while claiming it is available.
    #[test]
    #[serial]
    fn probed_detection_marks_a_dead_ollama_unavailable() {
        let port = {
            let l = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
            l.local_addr().unwrap().port()
        };
        let _host = EnvVarGuard::set("OLLAMA_HOST", format!("http://127.0.0.1:{port}"));

        let detected = detect_isolated(&ChatConfig::default(), true);
        let ollama = detected
            .iter()
            .find(|p| p.provider_type == "ollama")
            .expect("Ollama stays listed so the wizard can still offer it");

        assert!(
            !ollama.available,
            "a dead endpoint must not claim available"
        );
        assert!(
            ollama.reason().contains("not answering"),
            "the reason must say why: {}",
            ollama.reason()
        );
    }

    #[test]
    #[serial]
    fn probed_detection_marks_a_live_ollama_available() {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let _host = EnvVarGuard::set(
            "OLLAMA_HOST",
            format!("http://{}", listener.local_addr().unwrap()),
        );

        let detected = detect_isolated(&ChatConfig::default(), true);
        let ollama = detected
            .iter()
            .find(|p| p.provider_type == "ollama")
            .unwrap();

        assert!(ollama.available);
    }
}
