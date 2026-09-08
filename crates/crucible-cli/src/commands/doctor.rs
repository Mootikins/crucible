use anyhow::Result;
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::config::CliConfig;
use crate::formatting::TextFormat;
use crate::output;
use crucible_core::config::{BackendType, OllamaTagsResponse};
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

pub async fn execute(config_path_override: Option<PathBuf>, format: TextFormat) -> Result<()> {
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

    let explicit_override = config_path_override.is_some();
    let config_path = config_path_override.unwrap_or_else(CliConfig::default_config_path);
    // Whichever of `init.luau` / `init.lua` is really there, so `cru doctor`
    // does not report a missing config beside the file the daemon is loading.
    let config_dir = config_path.parent().unwrap_or_else(|| Path::new("."));
    let init_lua_path = crucible_lua::source_files::init_file(config_dir)
        .ok()
        .flatten()
        .unwrap_or_else(|| config_dir.join("init.lua"));
    let mut loaded_config: Option<CliConfig> = None;

    // Check 2: Config. The config is `init.lua`, and nothing else — so
    // "missing" means that file is not there. It is judged by the isolated
    // evaluation below, which is the daemon's own construction; presence is
    // all this row claims.
    if init_lua_path.exists() {
        results.push(DoctorCheckResult {
            check_name: "Config".to_string(),
            status: "pass".to_string(),
            message: format!("Config found at {}", display_path(&init_lua_path)),
        });
    } else {
        results.push(DoctorCheckResult {
            check_name: "Config".to_string(),
            status: "fail".to_string(),
            message: format!(
                "Config missing at {}. Try: `cru config init`",
                display_path(&init_lua_path)
            ),
        });
    }

    // Check 2a: a `config.toml` the daemon no longer reads.
    if let Some(result) = retired_config_note(&config_path) {
        results.push(result);
    }

    // Check 2b: the isolated config evaluation — the same construction as
    // the daemon's boot, run in this process, its output only a report (the
    // one sanctioned dual evaluation). Skipped when there is no config at
    // all; Check 2 already failed that.
    if init_lua_path.exists() || explicit_override {
        let daemon_boot_hash = match DaemonClient::connect().await {
            Ok(client) => client
                .call("config.effective", serde_json::json!({}))
                .await
                .ok()
                .and_then(|resp| resp["boot_hash"].as_str().map(String::from)),
            Err(_) => None,
        };
        let paths_fn: crucible_daemon::daemon_plugins::PluginPathsFn =
            std::sync::Arc::new(|rtp: &[PathBuf]| {
                crucible_daemon::daemon_plugins::daemon_plugin_paths(rtp)
            });
        // An explicit `-C` names the config root to evaluate; without one
        // the boot resolves the default root itself, which is the same path
        // `config_path` holds.
        let eval_source = explicit_override.then(|| config_path.clone());
        let (eval_results, evaluated_config) =
            evaluate_config_check(eval_source, daemon_boot_hash, paths_fn).await;
        results.extend(eval_results);
        // The evaluated config is the effective one — the seed plus
        // whatever init.lua set — so the checks below diagnose what the
        // daemon would actually run with.
        if let Some(config) = evaluated_config {
            loaded_config = Some(config);
        }
    }

    // Check 3: Providers
    let provider_checks = check_providers(loaded_config.as_ref()).await;
    if provider_checks.is_empty() {
        results.push(DoctorCheckResult {
            check_name: "Providers".to_string(),
            status: "warn".to_string(),
            message: crate::commands::chat_preflight::no_providers_remedies().to_string(),
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
            message: format!("Kiln path is not a directory: {}", display_path(&kiln_path)),
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

    // Check 4b: a kiln-local `.crucible/config.toml` — a file the pre-Lua
    // templates generated and NOTHING has ever read (its writers registered
    // the same values with the daemon). The user who has one almost
    // certainly believes it works; saying the true fact plainly is what a
    // doctor is for. A report only: the file is theirs to move or delete.
    if let Some(result) = kiln_local_config_note(&kiln_path) {
        results.push(result);
    }

    // Check 5: Embeddings
    let ollama_embedding_available = provider_checks
        .iter()
        .any(|p| p.backend == BackendType::Ollama && p.reachable);

    if cfg!(feature = "fastembed") {
        results.push(DoctorCheckResult {
            check_name: "Embeddings".to_string(),
            status: "pass".to_string(),
            message: "Embeddings available (fastembed). See `cru models embeddings` for the \
                      catalog"
                .to_string(),
        });
    } else if ollama_embedding_available {
        results.push(DoctorCheckResult {
            check_name: "Embeddings".to_string(),
            status: "pass".to_string(),
            message: "Embeddings available (ollama)".to_string(),
        });
    } else {
        // The pointer, not only the fault. A user who reads "disabled" has no
        // next command; `cru models embeddings` is the one that leads to a
        // working setup.
        results.push(DoctorCheckResult {
            check_name: "Embeddings".to_string(),
            status: "warn".to_string(),
            message: "No embedding backend available (semantic search disabled). Run \
                      `cru models embeddings` to pick a local model"
                .to_string(),
        });
    }

    // Check 6: Plugins (only if daemon is running)
    if let Ok(client) = DaemonClient::connect().await {
        match client.plugin_list().await {
            Ok(plugins) => {
                results.push(DoctorCheckResult {
                    check_name: "Plugins".to_string(),
                    status: "pass".to_string(),
                    message: format!("{} plugin(s) loaded", plugins.len()),
                });
            }
            Err(e) => {
                results.push(DoctorCheckResult {
                    check_name: "Plugins".to_string(),
                    status: "warn".to_string(),
                    message: format!("Plugin check failed ({})", e),
                });
            }
        }
    }

    // Check 7: Kiln References, against the registries rather than the config.
    //
    // Both listings come from the daemon, which is the only layer that merges
    // the config and the state store. A daemon that cannot be reached means the
    // check did not run — reported as such, because silence here would read as
    // "no problems found".
    match registry_listings().await {
        Ok((projects, kilns)) => {
            let warnings = validate_kiln_references(&projects, &kilns);
            if warnings.is_empty() {
                if !projects.is_empty() {
                    let by_origin =
                        |origin: &str| projects.iter().filter(|p| p["origin"] == origin).count();
                    results.push(DoctorCheckResult {
                        check_name: "Kiln References".to_string(),
                        status: "pass".to_string(),
                        message: format!(
                            "All project kiln references are valid ({} registered, {} declared in config)",
                            by_origin("registered"),
                            by_origin("config"),
                        ),
                    });
                }
            } else {
                for warning in &warnings {
                    results.push(DoctorCheckResult {
                        check_name: "Kiln References".to_string(),
                        status: "warn".to_string(),
                        message: warning.clone(),
                    });
                }
            }
        }
        Err(e) => results.push(DoctorCheckResult {
            check_name: "Kiln References".to_string(),
            status: "warn".to_string(),
            message: format!("Could not read the registries from the daemon: {e}"),
        }),
    }

    // Check 8: Config validation. `loaded_config` is the evaluated config
    // when Check 2b produced one, so this row reports whether the daemon's
    // own construction yielded a config to check the rest against.
    if loaded_config.is_some() {
        results.push(DoctorCheckResult {
            check_name: "Config validation".to_string(),
            status: "pass".to_string(),
            message: "Config parsed and validated".to_string(),
        });
    } else if init_lua_path.exists() {
        // The file is there and produced nothing — Check 2b already said why.
        results.push(DoctorCheckResult {
            check_name: "Config validation".to_string(),
            status: "fail".to_string(),
            message: "Config file exists but failed validation (see Config check above)"
                .to_string(),
        });
    }

    // Check 9: the resolved runtime path, per asset kind.
    //
    // "Where does Crucible look for this?" used to have five answers in five
    // resolvers, and a user debugging a skill that would not load had nowhere
    // to see any of them. One list now, so a missing root is visible rather
    // than inferred from a plugin that silently did not appear.
    results.extend(runtime_path_checks(&loaded_config));

    let total_checks = results.len();

    match format {
        TextFormat::Json => {
            println!("{}", serde_json::to_string_pretty(&results)?);
        }
        TextFormat::Text => {
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
                    output::success(&format!("All {} checks passed.", total_checks));
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

/// The global `config.toml`, reported as the retired file it is.
///
/// It WAS read, as the seed under `init.lua`, and v0.30.0 warned at every
/// boot that it was deprecated. The reader is gone now, so a user who still
/// has one has settings that stopped applying — the one case where a row
/// must say "no longer read" rather than "never read". The kiln-local file
/// below is the opposite case and must not borrow this wording.
///
/// Only a file actually named `config.toml` qualifies: `-C` names the file
/// whose directory is the config root, and a user who points it at their
/// `init.lua` must not be told their config is retired.
fn retired_config_note(config_path: &Path) -> Option<DoctorCheckResult> {
    if config_path.file_name()? != "config.toml" || !config_path.exists() {
        return None;
    }
    Some(DoctorCheckResult {
        check_name: "Retired config".to_string(),
        status: "warn".to_string(),
        message: format!(
            "{} is no longer read — its settings no longer apply. \
             Run `cru config migrate` to move them into init.lua",
            display_path(config_path)
        ),
    })
}

/// The never-read kiln-local config, reported as the true fact it is.
///
/// `.crucible/config.toml` in a kiln was generated by the pre-Lua templates
/// and read by nothing — no seed path ever existed for it (the provider
/// selection went to the daemon's `llm.json` instead). It must NOT be
/// called deprecated: "deprecated" asserts it used to work, which is false,
/// and a diagnostic asserting what the system cannot know is the defect
/// this doctor exists to avoid.
fn kiln_local_config_note(kiln_path: &Path) -> Option<DoctorCheckResult> {
    let config_toml = kiln_path.join(".crucible").join("config.toml");
    if !config_toml.exists() {
        return None;
    }
    Some(DoctorCheckResult {
        check_name: "Kiln-local config".to_string(),
        status: "warn".to_string(),
        message: format!(
            "{} exists, but nothing reads it — its settings have never taken effect.              Kiln-specific Lua belongs in .crucible/init.lua; daemon configuration in              the global init.lua (providers register via `cru init`)",
            display_path(&config_toml)
        ),
    })
}

/// The isolated-evaluation check rows, and the effective config when the
/// evaluation produced one.
///
/// Runs `init.lua` through the boot's own construction
/// (`evaluate_boot_config_with_paths`) so the verdict cannot drift from
/// what the daemon does — a bare-VM evaluation would report a working
/// `require("<plugin>")` line as broken. The plugin-path resolution is a
/// parameter for the same reason it is one on the boot: a test injects
/// fixture directories instead of reaching the developer's real plugin
/// dirs.
///
/// `daemon_boot_hash` is the running daemon's boot-input hash, when a
/// daemon answered: a mismatch means the daemon booted on an older config.
async fn evaluate_config_check(
    config_file: Option<PathBuf>,
    daemon_boot_hash: Option<String>,
    plugin_paths: crucible_daemon::daemon_plugins::PluginPathsFn,
) -> (Vec<DoctorCheckResult>, Option<CliConfig>) {
    let mut results = Vec::new();
    let boot = match crucible_daemon::daemon_plugins::evaluate_boot_config_with_paths(
        config_file,
        None,
        None,
        plugin_paths,
    )
    .await
    {
        Ok(boot) => boot,
        Err(e) => {
            results.push(DoctorCheckResult {
                check_name: "Config evaluation".to_string(),
                status: "fail".to_string(),
                message: format!("Config does not evaluate: {e}"),
            });
            return (results, None);
        }
    };

    let init_lua = crucible_lua::source_files::init_file(&boot.config_root)
        .ok()
        .flatten()
        .unwrap_or_else(|| boot.config_root.join("init.lua"));
    match &boot.eval_error {
        Some(error) => results.push(DoctorCheckResult {
            check_name: "Config evaluation".to_string(),
            status: "fail".to_string(),
            message: format!(
                "{} fails in isolated evaluation: {error}. The daemon boots on the seed \
                 values instead",
                display_path(&init_lua)
            ),
        }),
        None if init_lua.exists() => results.push(DoctorCheckResult {
            check_name: "Config evaluation".to_string(),
            status: "pass".to_string(),
            message: format!("{} evaluates cleanly", display_path(&init_lua)),
        }),
        None => results.push(DoctorCheckResult {
            check_name: "Config evaluation".to_string(),
            status: "pass".to_string(),
            message: "No init.lua; the seed config evaluates cleanly".to_string(),
        }),
    }

    if let Some(daemon_hash) = daemon_boot_hash {
        if daemon_hash == boot.boot_hash {
            results.push(DoctorCheckResult {
                check_name: "Config freshness".to_string(),
                status: "pass".to_string(),
                message: "The running daemon booted on this config".to_string(),
            });
        } else {
            results.push(DoctorCheckResult {
                check_name: "Config freshness".to_string(),
                status: "warn".to_string(),
                message: "The config changed since the daemon started; run `cru daemon restart` \
                          to apply"
                    .to_string(),
            });
        }
    }

    (results, Some(boot.config))
}

/// One line per asset kind: how many roots exist, and how many were offered.
///
/// A kind with candidates but none on disk is a `warn`, not a `fail`: a root
/// that does not exist is a normal state — `runtime_roots` names the extracted
/// tree unconditionally — but a kind with NO candidates at all means the path
/// itself is empty, which is a real misconfiguration.
fn runtime_path_checks(config: &Option<CliConfig>) -> Vec<DoctorCheckResult> {
    use crucible_core::runtime_path::{search_paths, RuntimeAsset};

    let runtimepath: Vec<PathBuf> = config
        .as_ref()
        .map(|c| c.runtimepath.clone())
        .unwrap_or_default();
    let path = crucible_daemon::runtime_path::daemon_path(&runtimepath);

    RuntimeAsset::ALL
        .iter()
        .map(|asset| {
            let dirs = search_paths(*asset, &path);
            let present: Vec<&PathBuf> = dirs
                .iter()
                .filter(|d| d.path.is_dir())
                .map(|d| &d.path)
                .collect();
            let (status, message) = if dirs.is_empty() {
                (
                    "fail",
                    format!("{}: no directory is on the path at all", asset.subdir()),
                )
            } else if present.is_empty() {
                (
                    "warn",
                    format!(
                        "{}: {} candidates, none on disk: {}",
                        asset.subdir(),
                        dirs.len(),
                        dirs.iter()
                            .map(|d| d.path.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
            } else {
                (
                    "pass",
                    format!(
                        "{}: {} of {} on disk: {}",
                        asset.subdir(),
                        present.len(),
                        dirs.len(),
                        present
                            .iter()
                            .map(|p| p.display().to_string())
                            .collect::<Vec<_>>()
                            .join(", ")
                    ),
                )
            };
            DoctorCheckResult {
                check_name: format!("Runtime path: {}", asset.subdir()),
                status: status.to_string(),
                message,
            }
        })
        .collect()
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
                Ok(reply) => match reply.text().await {
                    Ok(body) => match probe_reply(provider.provider_type, &body) {
                        Ok(()) => (true, None),
                        Err(err) => (false, Some(err)),
                    },
                    Err(err) => (false, Some(err.to_string())),
                },
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
        url.set_path("/api/tags");
    }

    Ok(url)
}

/// Judge the body a provider probe came back with.
///
/// An Ollama probe hits `/api/tags`, so its body must parse into the shared
/// [`OllamaTagsResponse`]; a proxy or a different service on that port
/// answers 200 with something else, and that is not a reachable Ollama.
/// Other backends have no shared reply shape; an answer is enough.
fn probe_reply(backend: BackendType, body: &str) -> std::result::Result<(), String> {
    if backend != BackendType::Ollama {
        return Ok(());
    }
    serde_json::from_str::<OllamaTagsResponse>(body)
        .map(|_| ())
        .map_err(|e| format!("/api/tags reply is not an Ollama model list: {e}"))
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

/// The project and kiln registry listings, from the daemon.
///
/// One connection for both: they are two halves of one question, and asking
/// twice would let them come from two different daemon states.
async fn registry_listings() -> anyhow::Result<(Vec<serde_json::Value>, Vec<serde_json::Value>)> {
    let client = crate::common::daemon_client().await?;
    let projects = client.project_registry_list().await?;
    let kilns = client.kiln_registry_list().await?;
    Ok((
        projects["projects"].as_array().cloned().unwrap_or_default(),
        kilns["kilns"].as_array().cloned().unwrap_or_default(),
    ))
}

fn display_path(path: &Path) -> String {
    path.to_string_lossy().to_string()
}

/// Validate that every kiln a project references is one Crucible can resolve.
///
/// Takes the two REGISTRY listings rather than the config, because neither
/// question can be answered from the config any more. Projects registered by
/// `cru init` live in `projects.json`, and kilns registered by `cru kiln
/// register` live in `kilns.json` — so the old version, which read
/// `config.projects` against `config.resolved_kilns()`, had drifted from a
/// check into a false one: a config project referencing a registered kiln was
/// reported broken, and a registered project was not checked at all.
///
/// A check that cannot fail is worse than a missing check, because it ends the
/// investigation. This one looks where the entries actually are.
///
/// `projects` and `kilns` are the `projects` array of `project.registry_list`
/// and the `kilns` array of `kiln.registry_list`.
pub fn validate_kiln_references(
    projects: &[serde_json::Value],
    kilns: &[serde_json::Value],
) -> Vec<String> {
    let known: std::collections::BTreeSet<&str> = kilns
        .iter()
        .filter_map(|kiln| kiln["name"].as_str())
        .collect();

    let mut warnings = Vec::new();
    for project in projects {
        let project_name = project["name"].as_str().unwrap_or("<unnamed>");
        let origin = project["origin"].as_str().unwrap_or("unknown");
        for kiln_name in project["kiln_names"].as_array().into_iter().flatten() {
            let Some(kiln_name) = kiln_name.as_str() else {
                continue;
            };
            if !known.contains(kiln_name) {
                // The origin is named because the fix differs: an entry the
                // user authored is edited, an entry a command wrote is
                // re-registered or forgotten.
                warnings.push(format!(
                    "Project '{project_name}' ({origin}) references kiln '{kiln_name}', \
                     which no registered kiln answers to"
                ));
            }
        }
    }
    warnings
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ollama_probe_accepts_a_tags_body() {
        let body = r#"{"models":[{"name":"nomic-embed-text:latest","size":1}]}"#;
        assert_eq!(probe_reply(BackendType::Ollama, body), Ok(()));
    }

    #[test]
    fn ollama_probe_rejects_a_body_that_is_not_a_model_list() {
        let err = probe_reply(BackendType::Ollama, "<html>proxy</html>").unwrap_err();
        assert!(err.contains("/api/tags"), "{err}");
    }

    #[test]
    fn other_backends_accept_any_body() {
        assert_eq!(probe_reply(BackendType::OpenAI, "<html>"), Ok(()));
    }

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

    fn project(name: &str, origin: &str, kilns: &[&str]) -> serde_json::Value {
        serde_json::json!({
            "name": name,
            "origin": origin,
            "kiln_names": kilns,
        })
    }

    fn kiln(name: &str) -> serde_json::Value {
        serde_json::json!({ "name": name })
    }

    #[test]
    fn doctor_reports_missing_kiln_references() {
        let warnings =
            validate_kiln_references(&[project("test", "registered", &["nonexistent"])], &[]);

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("nonexistent"), "{}", warnings[0]);
        assert!(warnings[0].contains("test"), "{}", warnings[0]);
    }

    /// The origin is in the message because the fix differs: an entry the user
    /// authored is edited, an entry a command wrote is re-registered.
    #[test]
    fn a_broken_reference_names_the_layer_that_declared_the_project() {
        let warnings = validate_kiln_references(&[project("legacy", "config", &["gone"])], &[]);

        assert_eq!(warnings.len(), 1);
        assert!(warnings[0].contains("config"), "{}", warnings[0]);
    }

    /// The check that had drifted into a false one: a project referencing a
    /// kiln that lives in the STATE layer. Read against `config.resolved_kilns()`
    /// this was reported broken; read against the registry it is fine.
    #[test]
    fn a_kiln_registered_with_the_daemon_satisfies_a_reference() {
        let warnings = validate_kiln_references(
            &[project("myproject", "registered", &["vault"])],
            &[kiln("vault")],
        );

        assert!(warnings.is_empty(), "{warnings:?}");
    }

    /// Fixture plugin-path resolution: no directories at all, so the
    /// evaluation cannot reach the developer's real plugin dirs.
    fn no_plugin_paths() -> crucible_daemon::daemon_plugins::PluginPathsFn {
        std::sync::Arc::new(|_: &[std::path::PathBuf]| Vec::new())
    }

    fn eval_row(results: &[DoctorCheckResult]) -> &DoctorCheckResult {
        results
            .iter()
            .find(|r| r.check_name == "Config evaluation")
            .expect("an evaluation row")
    }

    /// An `init.lua` that RAISES fails open in the daemon, so doctor reports
    /// the error and still hands the seed config to the checks below it.
    #[tokio::test]
    async fn a_raising_init_lua_fails_the_evaluation_check_by_name() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("init.lua"), "error('boom')").unwrap();

        let (results, config) = evaluate_config_check(
            Some(tmp.path().join("config.toml")),
            None,
            no_plugin_paths(),
        )
        .await;

        let row = eval_row(&results);
        assert_eq!(row.status, "fail");
        assert!(
            row.message.contains("init.lua") && row.message.contains("isolated evaluation"),
            "the failure must name the file and the step: {}",
            row.message
        );
        assert!(
            config.is_some(),
            "the fail-open seed config still comes back for the checks below"
        );
    }

    /// An `init.lua` that does not PARSE stops the boot, so there is no
    /// config to hand on. Doctor's job is then to say which line, because a
    /// user who cannot start the daemon runs doctor to find out why.
    #[tokio::test]
    async fn an_unparseable_init_lua_fails_the_evaluation_check_and_names_the_line() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(tmp.path().join("init.lua"), "this is not lua(").unwrap();

        let (results, config) = evaluate_config_check(
            Some(tmp.path().join("config.toml")),
            None,
            no_plugin_paths(),
        )
        .await;

        let row = eval_row(&results);
        assert_eq!(row.status, "fail");
        assert!(
            row.message.contains("init.lua:1"),
            "the failure must name the file and the line: {}",
            row.message
        );
        assert!(
            config.is_none(),
            "a config file that does not parse yields no config to check further"
        );
    }

    #[tokio::test]
    async fn a_clean_init_lua_passes_and_its_values_reach_the_returned_config() {
        let tmp = tempfile::TempDir::new().unwrap();
        std::fs::write(
            tmp.path().join("init.lua"),
            "cru.config.set{ chat = { model = 'from-lua' } }",
        )
        .unwrap();

        let (results, config) = evaluate_config_check(
            Some(tmp.path().join("config.toml")),
            None,
            no_plugin_paths(),
        )
        .await;

        assert_eq!(eval_row(&results).status, "pass");
        assert_eq!(
            config.unwrap().chat.model.as_deref(),
            Some("from-lua"),
            "the returned config must be the EVALUATED one, not the seed"
        );
    }

    /// The sanctioned dual evaluation: doctor diffs its own hash against
    /// the running daemon's, and only a mismatch warns.
    #[tokio::test]
    async fn the_freshness_row_warns_only_on_a_hash_mismatch() {
        let tmp = tempfile::TempDir::new().unwrap();
        // The `-C` path names the config ROOT through its directory; the
        // file itself is not read.
        let config_toml = tmp.path().join("config.toml");
        std::fs::write(tmp.path().join("init.lua"), "-- fine").unwrap();
        let current = crucible_daemon::daemon_plugins::boot_input_hash(&config_toml);

        let (results, _) =
            evaluate_config_check(Some(config_toml.clone()), Some(current), no_plugin_paths())
                .await;
        let fresh = results
            .iter()
            .find(|r| r.check_name == "Config freshness")
            .expect("a freshness row");
        assert_eq!(fresh.status, "pass");

        let (results, _) = evaluate_config_check(
            Some(config_toml),
            Some("stale-hash".to_string()),
            no_plugin_paths(),
        )
        .await;
        let fresh = results
            .iter()
            .find(|r| r.check_name == "Config freshness")
            .expect("a freshness row");
        assert_eq!(fresh.status, "warn");
        assert!(
            fresh.message.contains("cru daemon restart"),
            "the warning must name the remedy: {}",
            fresh.message
        );
    }

    /// The kiln-local `.crucible/config.toml` was never read by anything;
    /// doctor states that fact — and must not call it deprecated, which
    /// would assert it once worked.
    #[test]
    fn a_kiln_local_config_toml_is_reported_as_never_read() {
        let tmp = tempfile::TempDir::new().unwrap();
        assert!(
            kiln_local_config_note(tmp.path()).is_none(),
            "no file, no row"
        );

        let crucible_dir = tmp.path().join(".crucible");
        std::fs::create_dir_all(&crucible_dir).unwrap();
        std::fs::write(crucible_dir.join("config.toml"), "[chat]\n").unwrap();

        let row = kiln_local_config_note(tmp.path()).expect("a row for the existing file");
        assert_eq!(row.status, "warn");
        assert!(
            row.message.contains("nothing reads it") && row.message.contains("never taken effect"),
            "the row must state the true fact: {}",
            row.message
        );
        assert!(
            !row.message.to_lowercase().contains("deprecated"),
            "'deprecated' would claim the file once worked: {}",
            row.message
        );
    }

    /// The global `config.toml` was read until v0.30.0 and is not read now.
    /// A user with one is a user whose settings silently stopped applying,
    /// so the row must say that and name the command that ends it.
    #[test]
    fn a_retired_global_config_toml_is_reported_with_the_migrate_command() {
        let tmp = tempfile::TempDir::new().unwrap();
        let config_toml = tmp.path().join("config.toml");
        assert!(
            retired_config_note(&config_toml).is_none(),
            "no file, no row"
        );

        std::fs::write(&config_toml, "[chat]\n").unwrap();
        let row = retired_config_note(&config_toml).expect("a row for the existing file");
        assert_eq!(row.status, "warn");
        assert!(
            row.message.contains("cru config migrate"),
            "the row must name the remedy: {}",
            row.message
        );
        assert!(
            row.message.contains("no longer read"),
            "the row must say what changed: {}",
            row.message
        );

        // An `init.lua` the user named through `-C` is not a retired file.
        let init_lua = tmp.path().join("init.lua");
        std::fs::write(&init_lua, "").unwrap();
        assert!(retired_config_note(&init_lua).is_none());
    }

    #[test]
    fn doctor_no_warnings_when_no_projects() {
        assert!(validate_kiln_references(&[], &[]).is_empty());
    }
}
