use anyhow::Result;
use reqwest::Url;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::CliConfig;
use crate::output;
use crucible_config::BackendType;
use crucible_daemon::rpc_client::DaemonClient;

const PROVIDER_TIMEOUT_SECS: u64 = 2;

struct ProviderCheck {
    key: String,
    backend: BackendType,
    reachable: bool,
    detail: Option<String>,
}

pub async fn execute(config_path_override: Option<PathBuf>) -> Result<()> {
    output::header("Crucible Doctor - Installation Health Check");

    let mut failures = 0usize;
    let mut warnings = 0usize;

    match DaemonClient::connect().await {
        Ok(_) => output::success("Daemon running"),
        Err(_) => {
            output::error("Daemon not running");
            println!("    Try: `cru daemon start`");
            failures += 1;
        }
    }

    let config_path = config_path_override.unwrap_or_else(CliConfig::default_config_path);
    let mut loaded_config: Option<CliConfig> = None;

    if !config_path.exists() {
        output::error(&format!("Config missing at {}", display_path(&config_path)));
        println!("    Try: `cru config init`");
        failures += 1;
    } else {
        match CliConfig::load(Some(config_path.clone()), None, None) {
            Ok(config) => {
                output::success(&format!("Config found at {}", display_path(&config_path)));
                loaded_config = Some(config);
            }
            Err(err) => {
                output::warning(&format!("Config has errors: {}", err));
                println!("    Try: `cru config init` to repair your configuration");
                warnings += 1;
            }
        }
    }

    let provider_checks = check_providers(loaded_config.as_ref()).await;
    if provider_checks.is_empty() {
        output::warning("No LLM providers configured");
        println!("    Try: `cru config init`");
        warnings += 1;
    } else {
        for provider in &provider_checks {
            let label = format!("{} ({})", provider.key, provider.backend.as_str());
            if provider.reachable {
                output::success(&format!("Provider reachable: {}", label));
            } else {
                if let Some(detail) = &provider.detail {
                    output::error(&format!("Provider unreachable: {} ({})", label, detail));
                } else {
                    output::error(&format!("Provider unreachable: {}", label));
                }
                failures += 1;
            }
        }
    }

    let kiln_path = loaded_config
        .as_ref()
        .map(|cfg| cfg.kiln_path.clone())
        .unwrap_or_else(|| CliConfig::default().kiln_path);

    if !kiln_path.exists() {
        output::error(&format!("Kiln missing at {}", display_path(&kiln_path)));
        println!("    Try: `cru init`");
        failures += 1;
    } else if !kiln_path.is_dir() {
        output::error(&format!(
            "Kiln path is not a directory: {}",
            display_path(&kiln_path)
        ));
        failures += 1;
    } else if is_writable_dir(&kiln_path) {
        output::success(&format!("Kiln accessible at {}", display_path(&kiln_path)));
    } else {
        output::warning(&format!(
            "Kiln is read-only at {}",
            display_path(&kiln_path)
        ));
        warnings += 1;
    }

    let ollama_embedding_available = provider_checks
        .iter()
        .any(|p| p.backend == BackendType::Ollama && p.reachable);

    if cfg!(feature = "fastembed") {
        output::success("Embeddings available (fastembed)");
    } else if ollama_embedding_available {
        output::success("Embeddings available (ollama)");
    } else {
        output::warning("No embedding backend available (semantic search disabled)");
        warnings += 1;
    }

    println!();
    if failures == 0 {
        if warnings == 0 {
            output::success("All 5 checks passed.");
        } else {
            output::warning(&format!(
                "All checks passed with {} warning{}.",
                warnings,
                if warnings == 1 { "" } else { "s" }
            ));
        }
        return Ok(());
    }

    output::error(&format!(
        "{} check{} failed, {} warning{}.",
        failures,
        if failures == 1 { "" } else { "s" },
        warnings,
        if warnings == 1 { "" } else { "s" }
    ));
    std::process::exit(1);
}

async fn check_providers(config: Option<&CliConfig>) -> Vec<ProviderCheck> {
    let Some(config) = config else {
        return Vec::new();
    };

    if config.llm.providers.is_empty() {
        return Vec::new();
    }

    let client = reqwest::Client::builder()
        .timeout(Duration::from_secs(PROVIDER_TIMEOUT_SECS))
        .build()
        .ok();

    let mut checks = Vec::with_capacity(config.llm.providers.len());
    for (key, provider) in &config.llm.providers {
        let endpoint = provider.endpoint();
        let url = provider_health_url(provider.provider_type, &endpoint);

        let (reachable, detail) = match (&client, url) {
            (Some(http), Ok(url)) => match http.get(url.clone()).send().await {
                Ok(_) => (true, None),
                Err(err) => (false, Some(err.to_string())),
            },
            (None, _) => (false, Some("failed to initialize HTTP client".to_string())),
            (_, Err(err)) => (false, Some(err)),
        };

        checks.push(ProviderCheck {
            key: key.clone(),
            backend: provider.provider_type,
            reachable,
            detail,
        });
    }

    checks
}

fn provider_health_url(backend: BackendType, endpoint: &str) -> std::result::Result<Url, String> {
    let normalized = if endpoint.starts_with("http://") || endpoint.starts_with("https://") {
        endpoint.to_string()
    } else {
        format!("http://{}", endpoint)
    };

    let mut url = Url::parse(&normalized).map_err(|e| e.to_string())?;
    if backend == BackendType::Ollama {
        let path = url.path().trim_end_matches('/');
        if path.ends_with("/v1") {
            let trimmed = path.trim_end_matches("/v1");
            let updated = if trimmed.is_empty() { "/" } else { trimmed };
            url.set_path(updated);
        }
        url.set_path("/api/tags");
    }

    Ok(url)
}

fn is_writable_dir(path: &Path) -> bool {
    let probe = path.join(format!(".crucible-doctor-write-{}", std::process::id()));
    match std::fs::OpenOptions::new()
        .create_new(true)
        .write(true)
        .open(&probe)
    {
        Ok(_) => {
            let _ = std::fs::remove_file(probe);
            true
        }
        Err(_) => false,
    }
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}
