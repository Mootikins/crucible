//! `cru models embeddings` — the local embedding catalog, and how to change it.
//!
//! Three commands over one RPC method. The daemon owns the catalog because it
//! links fastembed and holds the model cache; this module renders what the
//! daemon reports, and writes the one config key that names the model.
//!
//! **This module matches no name.** A name the user types goes to the daemon,
//! which resolves it against the catalog and answers with the canonical form.
//! A copy of the matcher here would be a second authority on which names are
//! valid, and the two would drift apart — the first copy already refused
//! `BGESmallENV15`, which the catalog accepts.
//!
//! The write is deliberate and narrow. `use` saves `enrichment.provider` and
//! nothing else, and it says what it replaced, because a stored vector carries
//! the model that made it: the old vectors do not become the new model's
//! vectors, so a reprocess must follow.
//!
//! **The write goes through the daemon**, as `config.save`, which merges the
//! two keys into the running store and persists them in `settings.json`. It
//! used to edit `config.toml` directly. Nothing reads that file, so the model
//! never changed and the next `cru process` re-embedded with the old one —
//! while the command printed the path it had just written and reported
//! success.

use anyhow::{Context, Result};
use colored::Colorize;
use crucible_daemon::rpc_client::{EmbeddingCatalog, EmbeddingModelRow};

use crate::common::daemon_client;
use crate::formatting::OutputFormat;
use crate::output;

/// `cru models embeddings` — the catalog as a table, or as JSON.
pub async fn list(format: Option<OutputFormat>) -> Result<()> {
    let format = OutputFormat::for_stdout(format);
    let catalog = fetch(None, false).await?;

    match format {
        OutputFormat::Json => println!("{}", serde_json::to_string_pretty(&catalog)?),
        OutputFormat::Table => {
            println!("{}", table(&catalog));
            print_footer(&catalog);
        }
        OutputFormat::Plain => {
            for model in &catalog.models {
                println!("{}", model.name);
            }
        }
    }
    Ok(())
}

/// `cru models embeddings download <name>` — fetch one model into the cache.
///
/// The daemon refuses an unknown name before it touches the network, so a typo
/// costs one call and comes back with the near catalog names.
pub async fn download(name: &str, format: Option<OutputFormat>) -> Result<()> {
    // The daemon writes fastembed's progress bar to its own stdout, which is a
    // log file. So the wait is silent, and this line is what tells the user
    // that a silence of several minutes is the expected shape of a download.
    output::info(&format!(
        "Fetching {name} into the model cache. A large model takes minutes on a slow link, \
         and the daemon reports nothing until it finishes."
    ));
    let catalog = fetch(Some(name), true).await?;
    let name = catalog.resolved.clone().unwrap_or_else(|| name.to_string());
    let path = catalog.downloaded_to.as_deref();

    if format == Some(OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "model": name,
                "path": path,
                "bytes": catalog.downloaded_bytes,
            }))?
        );
        return Ok(());
    }

    output::success(&format!("{name} is in the cache."));
    println!("  Path: {}", path.unwrap_or("(the model cache)"));
    if let Some(bytes) = catalog.downloaded_bytes {
        println!("  Size: {}", human_bytes(bytes));
    }
    println!(
        "  {}",
        format!("Select it with: cru models embeddings use {name}").dimmed()
    );
    Ok(())
}

/// `cru models embeddings use <name>` — save the model as a durable preference.
///
/// The daemon resolves the name before the write, so a typo never lands in the
/// store and an alias lands as the canonical name. The write is a
/// `config.save`: the daemon merges the two keys into the live store and
/// writes them to `settings.json`, so the selection outlives the process and
/// applies to whoever reads the config next.
///
/// A leaf a human pinned in `init.lua` is REFUSED, with the file and line.
/// That refusal is the point of the verb: a save the boot would shadow acts
/// nowhere, and the command that silently lost it was the previous bug in
/// this function wearing a different hat.
pub async fn select(name: &str, format: Option<OutputFormat>) -> Result<()> {
    let catalog = fetch(Some(name), false).await?;
    let name = catalog
        .resolved
        .clone()
        .context("The daemon resolved no model for that name")?;
    let row = catalog.models.iter().find(|model| model.name == name);

    // The old value is the one that was in force, which is not always the one
    // a file held: an absent key still selects a model.
    let effective = catalog.configured.clone();
    let client = daemon_client().await?;
    let stale = stale_provider_keys(&client).await;
    let saved = client
        .call(
            "config.save",
            serde_json::json!({ "values": fastembed_provider_delta(&name) }),
        )
        .await
        .context("Failed to save the embedding model through the daemon")?;

    // A refusal is per leaf and carries the pin. Reporting success over it
    // would leave the user with the old model and a message saying otherwise.
    if let Some(refused) = saved["refused"].as_array().filter(|rows| !rows.is_empty()) {
        anyhow::bail!(
            "The embedding model is set in your config file, which outranks a saved \
             preference. Edit it there instead: {}",
            refused
                .iter()
                .map(describe_pin)
                .collect::<Vec<_>>()
                .join(", ")
        );
    }

    let previous = match effective.as_deref() {
        Some(model) if model == name => format!("{model} (unchanged)"),
        Some(model) => model.to_string(),
        None => "(none)".to_string(),
    };
    let needs_reprocess = effective.as_deref() != Some(name.as_str());

    if format == Some(OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "model": name,
                "previous": previous,
                "stale_keys": stale,
                "needs_reprocess": needs_reprocess,
                "downloaded": row.is_some_and(|row| row.downloaded),
            }))?
        );
        return Ok(());
    }

    println!(
        "  Embedding model: {} -> {}",
        previous.yellow(),
        name.green()
    );
    if !stale.is_empty() {
        output::warning(&format!(
            "The old provider was not fastembed. These keys are still in the config and nothing \
             reads them now: {}.",
            stale.join(", ")
        ));
    }
    if needs_reprocess {
        output::warning(
            "The stored vectors come from the old model. Semantic search stays wrong until \
             you rebuild them.",
        );
        // The saved value reaches the store at once, but the running pipeline
        // holds the embedder it was built with, so a reprocess before the
        // restart re-embeds every note with the *old* model and reports
        // success.
        println!("  {}", "Run: cru daemon restart".dimmed());
        println!("  {}", "Then: cru process --force".dimmed());
    }
    if !row.is_some_and(|row| row.downloaded) {
        println!(
            "  {}",
            format!("Not in the cache yet. Run: cru models embeddings download {name}").dimmed()
        );
    }
    Ok(())
}

/// The two keys `use` saves, as one `config.save` overlay.
///
/// `type` travels with `model` because `EmbeddingProviderConfig` is a tagged
/// union: a `model` saved under a `type = "openai"` block would name an
/// OpenAI model, not a local one.
fn fastembed_provider_delta(model: &str) -> serde_json::Value {
    serde_json::json!({
        "enrichment": {
            "provider": { "type": "fastembed", "model": model }
        }
    })
}

/// One refused leaf, as the sentence a user can act on.
fn describe_pin(row: &serde_json::Value) -> String {
    let key = row["key"].as_str().unwrap_or("the key");
    match (row["file"].as_str(), row["line"].as_u64()) {
        (Some(file), Some(line)) => format!("{key} at {file}:{line}"),
        (Some(file), None) => format!("{key} in {file}"),
        _ => key.to_string(),
    }
}

/// The keys the CURRENT provider block holds that fastembed does not read.
///
/// They stay in the config. Deleting them would throw away an API key the
/// user may want back, and `EmbeddingProviderConfig` ignores them without a
/// word, so the command is the only thing that can say so. Read from the
/// daemon's effective config rather than from one file, because the block can
/// be assembled from more than one layer.
///
/// An unreachable or unparseable answer yields no keys: the warning is a
/// courtesy, and failing the selection over it would be worse than omitting
/// it.
async fn stale_provider_keys(client: &crucible_daemon::rpc_client::DaemonClient) -> Vec<String> {
    let Ok(response) = client.call("config.effective", serde_json::json!({})).await else {
        return Vec::new();
    };
    let Some(provider) = response.pointer("/config/enrichment/provider") else {
        return Vec::new();
    };
    let kind = provider["type"].as_str().unwrap_or("fastembed");
    if kind.eq_ignore_ascii_case("fastembed") {
        return Vec::new();
    }
    provider
        .as_object()
        .map(|table| {
            table
                .keys()
                .filter(|key| *key != "type" && *key != "model")
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Ask the daemon for the catalog, optionally resolving or downloading a model.
async fn fetch(model: Option<&str>, download: bool) -> Result<EmbeddingCatalog> {
    let client = daemon_client().await?;
    let catalog = client
        .embedding_models(model, download)
        .await
        .context("Failed to read the embedding catalog from the daemon")?;
    if catalog.models.is_empty() {
        anyhow::bail!(
            "This daemon runs no local embedding models. It was built without the `fastembed` \
             feature. Configure a remote service under [enrichment.provider] instead."
        );
    }
    Ok(catalog)
}

/// The catalog as one table.
fn table(catalog: &EmbeddingCatalog) -> String {
    let rows: Vec<Vec<String>> = catalog
        .models
        .iter()
        .map(|model| {
            vec![
                marks(model, catalog.configured.as_deref()),
                model.name.clone(),
                model.dimensions.to_string(),
                model
                    .parameter_millions
                    .map_or_else(String::new, |m| format!("{m}M")),
                model
                    .max_input_tokens
                    .map_or_else(String::new, |t| t.to_string()),
                model
                    .retrieval_score
                    .map_or_else(String::new, |s| format!("{s:.2}")),
                model.note.clone(),
            ]
        })
        .collect();
    output::records_table(
        &["", "Model", "Dims", "Params", "Context", "MTEB", "Note"],
        &rows,
    )
}

/// The three one-character marks a row can carry.
fn marks(model: &EmbeddingModelRow, configured: Option<&str>) -> String {
    let mut marks = String::new();
    marks.push(if configured == Some(model.name.as_str()) {
        '*'
    } else {
        ' '
    });
    marks.push(if model.curated { '+' } else { ' ' });
    marks.push(if model.downloaded { 'v' } else { ' ' });
    marks
}

/// The legend and the cache path, under the table.
fn print_footer(catalog: &EmbeddingCatalog) {
    println!(
        "{}",
        "  * configured   + curated, and downloadable   v in the cache   \
         MTEB: v1 English retrieval, nDCG@10"
            .dimmed()
    );
    if let Some(dir) = &catalog.cache_dir {
        println!("{}", format!("  Cache: {dir}").dimmed());
    }
}

/// Bytes as a short human string.
fn human_bytes(bytes: u64) -> String {
    #[allow(clippy::cast_precision_loss)]
    let mb = bytes as f64 / 1_048_576.0;
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{mb:.0} MB")
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    use crucible_core::config::{CliAppConfig, EmbeddingProviderConfig};

    /// The saved overlay must name leaves the config type really has.
    ///
    /// The expectation comes from the running types, not from a copy of the
    /// key names here: the overlay is deserialized as a `CliAppConfig`, and
    /// its provider as the tagged union the daemon stores. A renamed path or
    /// a wrong tag stops parsing instead of saving a leaf nothing reads —
    /// which is exactly what the `config.toml` write did.
    #[test]
    fn the_saved_overlay_deserializes_as_a_fastembed_provider() {
        let delta = fastembed_provider_delta("arctic-embed-m");

        let config: CliAppConfig =
            serde_json::from_value(delta.clone()).expect("the overlay must be app config");
        let provider = config
            .enrichment
            .expect("the overlay must reach `enrichment`")
            .provider;
        assert!(
            matches!(&provider, EmbeddingProviderConfig::FastEmbed(cfg) if cfg.model == "arctic-embed-m"),
            "the overlay must select fastembed and the named model: {provider:?}"
        );
    }

    /// The overlay carries the two keys and no third one.
    ///
    /// `config.save` writes what it is given, whole. A `cache_dir` or a
    /// `batch_size` filled in from a default here would freeze that default
    /// into the user's `settings.json` and outrank a plugin that sets it.
    #[test]
    fn the_saved_overlay_carries_only_the_two_keys() {
        let delta = fastembed_provider_delta("bge-small-en-v1.5");
        let provider = delta
            .pointer("/enrichment/provider")
            .and_then(serde_json::Value::as_object)
            .expect("the overlay names enrichment.provider");

        let mut keys: Vec<&str> = provider.keys().map(String::as_str).collect();
        keys.sort_unstable();
        assert_eq!(keys, ["model", "type"]);
        assert_eq!(delta.as_object().map(serde_json::Map::len), Some(1));
    }

    /// A refusal must name the file and line that holds the key, because that
    /// is the only place the user can change it.
    #[test]
    fn a_refused_leaf_is_described_with_its_file_and_line() {
        let row = serde_json::json!({
            "key": "enrichment.provider.model",
            "source": "lua",
            "file": "/home/u/.config/crucible/init.lua",
            "line": 12,
        });
        assert_eq!(
            describe_pin(&row),
            "enrichment.provider.model at /home/u/.config/crucible/init.lua:12"
        );
    }
}
