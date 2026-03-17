use anyhow::Result;
use reqwest::Url;
use serde::{Deserialize, Serialize};
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

/// A single doctor check result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DoctorCheckResult {
    pub check_name: String,
    pub status: String, // "pass", "fail", "warn"
    pub message: String,
}

pub async fn execute(config_path_override: Option<PathBuf>, format: &str) -> Result<()> {
    let mut results = Vec::new();

    // Check 1: Daemon
    match DaemonClient::connect().await {
        Ok(_) => {
            results.push(DoctorCheckResult {
                check_name: "Daemon".to_string(),
                status: "pass".to_string(),
                message: "Daemon running".to_string(),
            });
        }
        Err(_) => {
            results.push(DoctorCheckResult {
                check_name: "Daemon".to_string(),
                status: "fail".to_string(),
                message: "Daemon not running. Try: `cru daemon start`".to_string(),
            });
        }
    }

    let config_path = config_path_override.unwrap_or_else(CliConfig::default_config_path);
    let mut loaded_config: Option<CliConfig> = None;

    // Check 2: Config
    if !config_path.exists() {
        results.push(DoctorCheckResult {
            check_name: "Config".to_string(),
            status: "fail".to_string(),
            message: format!(
                "Config missing at {}. Try: `cru config init`",
                display_path(&config_path)
            ),
        });
    } else {
        match CliConfig::load(Some(config_path.clone()), None, None) {
            Ok(config) => {
                results.push(DoctorCheckResult {
                    check_name: "Config".to_string(),
                    status: "pass".to_string(),
                    message: format!("Config found at {}", display_path(&config_path)),
                });
                loaded_config = Some(config);
            }
            Err(err) => {
                results.push(DoctorCheckResult {
                    check_name: "Config".to_string(),
                    status: "warn".to_string(),
                    message: format!(
                        "Config has errors: {}. Try: `cru config init` to repair",
                        err
                    ),
                });
            }
        }
    }

    // Check 3: Providers
    let provider_checks = check_providers(loaded_config.as_ref()).await;
    if provider_checks.is_empty() {
        results.push(DoctorCheckResult {
            check_name: "Providers".to_string(),
            status: "warn".to_string(),
            message: "No LLM providers configured. Try: `cru config init`".to_string(),
        });
    } else {
        let mut all_reachable = true;
        for provider in &provider_checks {
            let label = format!("{} ({})", provider.key, provider.backend.as_str());
            if !provider.reachable {
                all_reachable = false;
                let detail = provider
                    .detail
                    .as_ref()
                    .map(|d| format!(" ({})", d))
                    .unwrap_or_default();
                results.push(DoctorCheckResult {
                    check_name: format!("Provider: {}", label),
                    status: "fail".to_string(),
                    message: format!("Provider unreachable{}", detail),
                });
            }
        }
        if all_reachable {
            results.push(DoctorCheckResult {
                check_name: "Providers".to_string(),
                status: "pass".to_string(),
                message: format!("All {} provider(s) reachable", provider_checks.len()),
            });
        }
    }

    // Check 4: Kiln
    let kiln_path = loaded_config
        .as_ref()
        .map(|cfg| cfg.kiln_path.clone())
        .unwrap_or_else(|| CliConfig::default().kiln_path);

    if !kiln_path.exists() {
        results.push(DoctorCheckResult {
            check_name: "Kiln".to_string(),
            status: "fail".to_string(),
            message: format!(
                "Kiln missing at {}. Try: `cru init`",
                display_path(&kiln_path)
            ),
        });
    } else if !kiln_path.is_dir() {
        results.push(DoctorCheckResult {
            check_name: "Kiln".to_string(),
            status: "fail".to_string(),
            message: format!(
                "Kiln path is not a directory: {}",
                display_path(&kiln_path)
            ),
        });
    } else if is_writable_dir(&kiln_path) {
        results.push(DoctorCheckResult {
            check_name: "Kiln".to_string(),
            status: "pass".to_string(),
            message: format!("Kiln accessible at {}", display_path(&kiln_path)),
        });
    } else {
        results.push(DoctorCheckResult {
            check_name: "Kiln".to_string(),
            status: "warn".to_string(),
            message: format!("Kiln is read-only at {}", display_path(&kiln_path)),
        });
    }

    // Check 5: Embeddings
    let ollama_embedding_available = provider_checks
        .iter()
        .any(|p| p.backend == BackendType::Ollama && p.reachable);

    if cfg!(feature = "fastembed") {
        results.push(DoctorCheckResult {
            check_name: "Embeddings".to_string(),
            status: "pass".to_string(),
            message: "Embeddings available (fastembed)".to_string(),
        });
    } else if ollama_embedding_available {
        results.push(DoctorCheckResult {
            check_name: "Embeddings".to_string(),
            status: "pass".to_string(),
            message: "Embeddings available (ollama)".to_string(),
        });
    } else {
        results.push(DoctorCheckResult {
            check_name: "Embeddings".to_string(),
            status: "warn".to_string(),
            message: "No embedding backend available (semantic search disabled)".to_string(),
        });
    }

    // Output results based on format
    match format {
        "json" => {
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        _ => {
            // Default table format
            output::header("Crucible Doctor - Installation Health Check");

            let mut failures = 0usize;
            let mut warnings = 0usize;

            for result in &results {
                match result.status.as_str() {
                    "pass" => output::success(&result.message),
                    "fail" => {
                        output::error(&result.message);
                        failures += 1;
                    }
                    "warn" => {
                        output::warning(&result.message);
                        warnings += 1;
                    }
                    _ => {}
                }
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
    }

    Ok(())
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
        let path = url.path().trim_end_matches('/').to_string();
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_doctor_check_result_serializes_to_json() {
        let result = DoctorCheckResult {
            check_name: "Daemon".to_string(),
            status: "pass".to_string(),
            message: "Daemon running".to_string(),
        };

        let json = serde_json::to_string(&result).unwrap();
        let parsed: DoctorCheckResult = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.check_name, "Daemon");
        assert_eq!(parsed.status, "pass");
        assert_eq!(parsed.message, "Daemon running");
    }

    #[test]
    fn test_doctor_results_array_serializes_to_json() {
        let results = vec![
            DoctorCheckResult {
                check_name: "Daemon".to_string(),
                status: "pass".to_string(),
                message: "Daemon running".to_string(),
            },
            DoctorCheckResult {
                check_name: "Config".to_string(),
                status: "fail".to_string(),
                message: "Config missing".to_string(),
            },
        ];

        let json = serde_json::to_string_pretty(&results).unwrap();
        let parsed: Vec<DoctorCheckResult> = serde_json::from_str(&json).unwrap();

        assert_eq!(parsed.len(), 2);
        assert_eq!(parsed[0].status, "pass");
        assert_eq!(parsed[1].status, "fail");
    }
}
