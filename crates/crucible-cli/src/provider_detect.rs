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

use crucible_core::config::credentials::{CredentialSource, CredentialStore, SecretsFile};
use crucible_core::config::{BackendType, ChatConfig, DEFAULT_OLLAMA_ENDPOINT};

/// A detected provider with availability info
#[derive(Debug, Clone)]
pub struct DetectedProvider {
    pub name: String,
    pub provider_type: String,
    pub available: bool,
    pub reason: String,
    pub default_model: Option<String>,
    pub source: Option<CredentialSource>,
}

/// Get the Ollama endpoint from OLLAMA_HOST env var or default
pub fn ollama_endpoint() -> String {
    std::env::var("OLLAMA_HOST")
        .ok()
        .map(|host| {
            // OLLAMA_HOST can be just "host:port" or a full URL
            if host.starts_with("http://") || host.starts_with("https://") {
                host
            } else {
                format!("http://{}", host)
            }
        })
        .unwrap_or_else(|| DEFAULT_OLLAMA_ENDPOINT.to_string())
}

/// Whether a credential for `provider` exists, and where it came from.
///
/// The store is a parameter so callers — and tests — say which one they mean:
/// the default `SecretsFile::new()` reads the developer's real
/// `~/.config/crucible/secrets.toml`, whose contents would otherwise leak into
/// assertions (pass on CI, fail on any box with stored credentials).
fn has_api_key_with_source_in(store: &SecretsFile, provider: &str) -> Option<CredentialSource> {
    // The env-var name comes from the backend's own metadata rather than a
    // hand-kept list here — hardcoding only OPENAI/ANTHROPIC was how
    // OpenRouter and Z.AI keys went undetected.
    let env_var = provider
        .to_lowercase()
        .parse::<BackendType>()
        .ok()
        .and_then(|b| b.api_key_env_var());
    if let Some(env_var) = env_var {
        if std::env::var(env_var).is_ok_and(|v| !v.trim().is_empty()) {
            return Some(CredentialSource::EnvVar);
        }
    }

    if let Ok(Some(_)) = store.get(provider) {
        return Some(CredentialSource::Store);
    }

    None
}

/// Chat backends that authenticate with an API key, in the order they are
/// offered when several have credentials.
const KEYED_CHAT_BACKENDS: &[BackendType] = &[
    BackendType::Anthropic,
    BackendType::OpenAI,
    BackendType::OpenRouter,
    BackendType::ZAI,
];

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
    let mut providers = Vec::new();

    for &backend in KEYED_CHAT_BACKENDS {
        let provider_type = backend.as_str();
        if let Some(src) = has_api_key_with_source_in(store, provider_type) {
            providers.push(DetectedProvider {
                name: keyed_backend_display_name(backend).to_string(),
                provider_type: provider_type.to_string(),
                available: true,
                reason: format!("API key found ({})", src),
                default_model: config
                    .model
                    .clone()
                    .or_else(|| backend.default_chat_model().map(str::to_string)),
                source: Some(src),
            });
        }
    }

    // Ollama is always offered: it is the no-credential path. `available` is
    // an assumption unless the caller asked for a probe. `OLLAMA_HOST=""`
    // counts as unset — the daemon's env discovery filters empty values, and
    // the two must agree or an empty export produces the endpoint "http://".
    let ollama_host_set = std::env::var("OLLAMA_HOST").is_ok_and(|v| !v.trim().is_empty());
    let endpoint = if ollama_host_set {
        ollama_endpoint()
    } else {
        config
            .endpoint
            .clone()
            .unwrap_or_else(|| DEFAULT_OLLAMA_ENDPOINT.to_string())
    };
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
        name: "Ollama (Local)".to_string(),
        provider_type: "ollama".to_string(),
        available,
        reason,
        default_model: config.model.clone(),
        source: None,
    });

    // Rank credential-backed providers ahead of the assumed local default,
    // and reachable ones ahead of dead ones. `cru init -y` picks from the
    // front, so without this a user whose only credential is
    // ANTHROPIC_API_KEY got an Ollama kiln. Stable sort preserves the
    // KEYED_CHAT_BACKENDS order within each group.
    providers.sort_by_key(|p| (p.source.is_none(), !p.available));

    providers
}

fn keyed_backend_display_name(backend: BackendType) -> &'static str {
    match backend {
        BackendType::Anthropic => "Anthropic",
        BackendType::OpenAI => "OpenAI",
        BackendType::OpenRouter => "OpenRouter",
        BackendType::ZAI => "Z.AI",
        _ => backend.as_str(),
    }
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
        assert!(detected[0].reason.contains("config provider=ollama"));
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
        assert!(ollama.reason.contains("OLLAMA_HOST"));
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
        has_api_key_with_source_in(&store, provider).is_some()
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
            name: "Test Provider".to_string(),
            provider_type: "test".to_string(),
            available: true,
            reason: "Test reason".to_string(),
            default_model: Some("test-model".to_string()),
            source: Some(CredentialSource::EnvVar),
        };

        assert_eq!(provider.name, "Test Provider");
        assert_eq!(provider.provider_type, "test");
        assert!(provider.available);
        assert_eq!(provider.reason, "Test reason");
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
            !ollama.reason.contains("OLLAMA_HOST"),
            "an empty OLLAMA_HOST must not be reported as the source: {}",
            ollama.reason
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
            ollama.reason.contains("not answering"),
            "the reason must say why: {}",
            ollama.reason
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

    #[test]
    #[serial]
    fn test_ollama_endpoint_default() {
        let _guard = EnvVarGuard::remove("OLLAMA_HOST");
        assert_eq!(ollama_endpoint(), "http://localhost:11434");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_custom_host_port() {
        let _guard = EnvVarGuard::set("OLLAMA_HOST", "myhost:11435".to_string());
        assert_eq!(ollama_endpoint(), "http://myhost:11435");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_full_url() {
        let _guard = EnvVarGuard::set("OLLAMA_HOST", "http://custom-ollama.local:8080".to_string());
        assert_eq!(ollama_endpoint(), "http://custom-ollama.local:8080");
    }

    #[test]
    #[serial]
    fn test_ollama_endpoint_https() {
        let _guard = EnvVarGuard::set(
            "OLLAMA_HOST",
            "https://secure-ollama.example.com".to_string(),
        );
        assert_eq!(ollama_endpoint(), "https://secure-ollama.example.com");
    }
}
