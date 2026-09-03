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
//! The write is deliberate and narrow. `use` touches `[enrichment.provider]`
//! in the user's own config file and nothing else, and it says what it
//! replaced, because a stored vector carries the model that made it: the old
//! vectors do not become the new model's vectors, so a reprocess must follow.

use anyhow::{Context, Result};
use colored::Colorize;
use crucible_daemon::rpc_client::{EmbeddingCatalog, EmbeddingModelRow};
use std::path::{Path, PathBuf};

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

/// `cru models embeddings use <name>` — name the model in the config file.
///
/// The daemon resolves the name before the write, so a typo never lands in the
/// user's file and an alias lands as the canonical name. The write touches two
/// keys and keeps every other line.
pub async fn select(
    name: &str,
    config_path: Option<PathBuf>,
    format: Option<OutputFormat>,
) -> Result<()> {
    let catalog = fetch(Some(name), false).await?;
    let name = catalog
        .resolved
        .clone()
        .context("The daemon resolved no model for that name")?;
    let row = catalog.models.iter().find(|model| model.name == name);

    // The old value is the one that was in force, which is not always the one
    // the file held: an absent key still selects a model.
    let effective = catalog.configured.clone();
    let path = config_path.unwrap_or_else(crucible_core::config::CliAppConfig::default_config_path);
    let replaced = write_model(&path, &name)?;
    let previous = match (replaced.model, effective.as_deref()) {
        (Some(from_file), _) => from_file,
        (None, Some(default)) => format!("{default} (the default)"),
        (None, None) => "(none)".to_string(),
    };
    let stale = replaced.stale_keys;
    let needs_reprocess = effective.as_deref() != Some(name.as_str());

    if format == Some(OutputFormat::Json) {
        println!(
            "{}",
            serde_json::to_string_pretty(&serde_json::json!({
                "config_file": path.display().to_string(),
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
        "{} {}",
        "Config file:".bold(),
        path.display().to_string().cyan()
    );
    println!(
        "  Embedding model: {} -> {}",
        previous.yellow(),
        name.green()
    );
    if !stale.is_empty() {
        output::warning(&format!(
            "The old provider was not fastembed. These keys are still in the file and nothing \
             reads them now: {}.",
            stale.join(", ")
        ));
    }
    if needs_reprocess {
        output::warning(
            "The stored vectors come from the old model. Semantic search stays wrong until \
             you rebuild them.",
        );
        // The running daemon holds the config it was bound with. It reads no
        // file again, so a reprocess before the restart re-embeds every note
        // with the *old* model and reports success.
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

/// What the config file held before `use` wrote over it.
#[derive(Debug)]
struct Replaced {
    /// The model the file named, when it named one.
    model: Option<String>,

    /// The keys the old provider block held that fastembed does not read.
    ///
    /// They stay in the file. Deleting them would throw away an API key the
    /// user may want back, and `EmbeddingProviderConfig` ignores them without
    /// a word, so the command says which ones are now dead.
    stale_keys: Vec<String>,
}

/// Write `[enrichment.provider]` into the config file. Returns what it replaced.
///
/// `toml_edit` keeps every other line of the file — comments included —
/// because the file is the user's, and a rewrite that reformatted it would be
/// a second, unasked-for change.
fn write_model(path: &Path, model: &str) -> Result<Replaced> {
    // A read that fails for any reason other than "no such file" must stop
    // the command. Treating an unreadable file as an empty one would replace
    // the whole config with the two keys below — a config holding one
    // non-UTF-8 byte is enough to trigger it.
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => return Err(e).with_context(|| format!("Failed to read {}", path.display())),
    };
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    let not_a_table = || {
        anyhow::anyhow!(
            "{} writes `enrichment` or `enrichment.provider` as an inline table or a value, \
             which this command cannot edit. Edit the file by hand.",
            path.display()
        )
    };
    let enrichment = document
        .entry("enrichment")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(not_a_table)?;
    // Implicit, so a file that gains only `[enrichment.provider]` does not
    // also gain an empty `[enrichment]` header above it. toml_edit still
    // writes the header once the table holds a value of its own.
    enrichment.set_implicit(true);
    let provider = enrichment
        .entry("provider")
        .or_insert(toml_edit::table())
        .as_table_mut()
        .ok_or_else(not_a_table)?;

    let string = |key: &str| {
        provider
            .get(key)
            .and_then(|v| v.as_str())
            .map(str::to_string)
    };
    let previous = string("model");
    let stale_keys = match string("type") {
        Some(kind) if !kind.eq_ignore_ascii_case("fastembed") => provider
            .iter()
            .map(|(key, _)| key.to_string())
            .filter(|key| key != "type" && key != "model")
            .collect(),
        _ => Vec::new(),
    };
    provider.insert("type", toml_edit::value("fastembed"));
    provider.insert("model", toml_edit::value(model));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    std::fs::write(path, document.to_string())
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(Replaced {
        model: previous,
        stale_keys,
    })
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
                format!("{}M", model.parameter_millions),
                model.max_input_tokens.to_string(),
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
    marks.push(if model.recommended { '+' } else { ' ' });
    marks.push(if model.downloaded { 'v' } else { ' ' });
    marks
}

/// The legend and the cache path, under the table.
fn print_footer(catalog: &EmbeddingCatalog) {
    println!(
        "{}",
        "  * configured   + recommended   v in the cache   MTEB: v1 English retrieval, nDCG@10"
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

    /// `use` writes both keys, keeps the rest of the file, and reports the old
    /// model. The path is a `TempDir`, so the developer's own config never
    /// takes part.
    #[test]
    fn use_writes_the_provider_keys_and_reports_the_old_model() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "# keep me\n[enrichment.provider]\ntype = \"fastembed\"\nmodel = \"bge-small-en-v1.5\"\n\n[chat]\nmodel = \"llama3\"\n",
        )
        .expect("seed config");

        let replaced = write_model(&path, "arctic-embed-m").expect("write");
        assert_eq!(replaced.model.as_deref(), Some("bge-small-en-v1.5"));
        assert!(replaced.stale_keys.is_empty());

        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(written.contains("model = \"arctic-embed-m\""), "{written}");
        assert!(written.contains("type = \"fastembed\""), "{written}");
        assert!(written.contains("# keep me"), "{written}");
        assert!(written.contains("[chat]"), "{written}");
        assert!(written.contains("llama3"), "{written}");
    }

    /// A config file that does not exist yet is created, and there is no old
    /// model to report.
    #[test]
    fn use_creates_a_missing_config_file() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("nested").join("config.toml");

        let replaced = write_model(&path, "bge-base-en-v1.5").expect("write");
        assert_eq!(replaced.model, None);
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("model = \"bge-base-en-v1.5\""),
            "{written}"
        );
    }

    /// A config file this command cannot read is not an empty config file.
    ///
    /// The read fails on the non-UTF-8 byte. Treating that as "the file holds
    /// nothing" would write two keys over the user's whole config, so the
    /// command must stop and leave every byte where it was.
    #[test]
    fn use_refuses_a_config_file_it_cannot_read() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("config.toml");
        let seeded = b"# caf\xe9\n[chat]\nmodel = \"llama3\"\n";
        std::fs::write(&path, seeded).expect("seed config");

        let error = write_model(&path, "arctic-embed-m").expect_err("an unreadable file stops it");
        assert!(
            error.to_string().contains("Failed to read"),
            "the refusal must say the read failed, but it said: {error}"
        );
        assert_eq!(
            std::fs::read(&path).expect("read back"),
            seeded,
            "the command rewrote a file it could not read"
        );
    }

    /// Switching away from a remote provider names the keys it left behind.
    ///
    /// The keys stay: one of them is an API key the user may want back. But
    /// nothing reads them under `type = "fastembed"`, and the config loader
    /// ignores them without a word, so the command is the only thing that can
    /// say so.
    #[test]
    fn use_reports_the_keys_the_old_provider_left_behind() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(
            &path,
            "[enrichment.provider]\ntype = \"openai\"\napi_key = \"sk-secret\"\nbase_url = \"https://example.invalid\"\nmodel = \"text-embedding-3-small\"\n",
        )
        .expect("seed config");

        let replaced = write_model(&path, "arctic-embed-m").expect("write");
        assert_eq!(replaced.model.as_deref(), Some("text-embedding-3-small"));
        assert_eq!(replaced.stale_keys, vec!["api_key", "base_url"]);

        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("sk-secret"),
            "the command deleted a key it only had to name: {written}"
        );
    }

    /// An inline `provider` table is refused, and the refusal says so.
    ///
    /// `toml_edit` gives no table to edit, and the previous sentence told the
    /// user the file held something that was "not a table" when it held one.
    #[test]
    fn use_refuses_an_inline_provider_table_and_says_why() {
        let dir = tempfile::TempDir::new().expect("temp dir");
        let path = dir.path().join("config.toml");
        std::fs::write(&path, "[enrichment]\nprovider = { type = \"openai\" }\n")
            .expect("seed config");

        let error = write_model(&path, "arctic-embed-m").expect_err("an inline table stops it");
        assert!(
            error.to_string().contains("inline table"),
            "the refusal must name the shape it cannot edit, but it said: {error}"
        );
    }
}
