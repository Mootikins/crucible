//! `cru models embeddings` — the local embedding catalog, and how to change it.
//!
//! Three commands over one RPC method. The daemon owns the catalog because it
//! links fastembed and holds the model cache; this module renders what the
//! daemon reports, and writes the one config key that names the model.
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
    let catalog = fetch(None).await?;

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
/// The name is checked against the catalog first, so a typo costs one cheap
/// call rather than a refusal that arrives wrapped in a download error.
pub async fn download(name: &str) -> Result<()> {
    let catalog = fetch(None).await?;
    let name = find(&catalog, name)?.name.clone();

    output::info(&format!("Fetching {name} into the model cache."));
    let after = fetch(Some(&name)).await?;
    let bytes = after
        .models
        .iter()
        .find(|model| model.name == name)
        .and_then(|model| model.disk_bytes);

    output::success(&format!("{name} is in the cache."));
    println!(
        "  Path: {}",
        after
            .downloaded_to
            .as_deref()
            .unwrap_or("(the model cache)")
    );
    if let Some(bytes) = bytes {
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
/// The catalog check comes before the write, so a typo never lands in the
/// user's file. The write touches two keys and keeps every other line.
pub async fn select(name: &str, config_path: Option<PathBuf>) -> Result<()> {
    let catalog = fetch(None).await?;
    let entry = find(&catalog, name)?;

    // The old value is the one that was in force, which is not always the one
    // the file held: an absent key still selects a model.
    let effective = catalog.configured.clone();
    let path = config_path.unwrap_or_else(crucible_core::config::CliAppConfig::default_config_path);
    let written_over = write_model(&path, &entry.name)?;
    let previous = match (written_over, effective.as_deref()) {
        (Some(from_file), _) => from_file,
        (None, Some(default)) => format!("{default} (the default)"),
        (None, None) => "(none)".to_string(),
    };

    println!(
        "{} {}",
        "Config file:".bold(),
        path.display().to_string().cyan()
    );
    println!(
        "  Embedding model: {} -> {}",
        previous.yellow(),
        entry.name.green()
    );
    if effective.as_deref() != Some(entry.name.as_str()) {
        output::warning(
            "The stored vectors come from the old model. Semantic search stays wrong until \
             you rebuild them.",
        );
        println!("  {}", "Run: cru process --force".dimmed());
    }
    if !entry.downloaded {
        println!(
            "  {}",
            format!(
                "Not in the cache yet. Run: cru models embeddings download {}",
                entry.name
            )
            .dimmed()
        );
    }
    Ok(())
}

/// The catalog row a user named, or a refusal that names the near entries.
fn find<'a>(catalog: &'a EmbeddingCatalog, name: &str) -> Result<&'a EmbeddingModelRow> {
    catalog
        .models
        .iter()
        .find(|model| {
            let wanted = name.trim();
            model.name.eq_ignore_ascii_case(wanted)
                || model
                    .aliases
                    .iter()
                    .any(|alias| alias.eq_ignore_ascii_case(wanted))
        })
        .ok_or_else(|| anyhow::anyhow!(unknown_model(name, catalog)))
}

/// Ask the daemon for the catalog, optionally downloading one model first.
async fn fetch(download: Option<&str>) -> Result<EmbeddingCatalog> {
    let client = daemon_client().await?;
    let catalog = client
        .embedding_models(download)
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

/// Write `[enrichment.provider]` into the config file. Returns the old model.
///
/// `toml_edit` keeps every other line of the file — comments included —
/// because the file is the user's, and a rewrite that reformatted it would be
/// a second, unasked-for change.
fn write_model(path: &Path, model: &str) -> Result<Option<String>> {
    let text = std::fs::read_to_string(path).unwrap_or_default();
    let mut document = text
        .parse::<toml_edit::DocumentMut>()
        .with_context(|| format!("Failed to parse {}", path.display()))?;

    let not_a_table = || {
        anyhow::anyhow!(
            "{} sets `enrichment` or `enrichment.provider` to something that is not a table",
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

    let previous = provider
        .get("model")
        .and_then(|v| v.as_str())
        .map(str::to_string);
    provider.insert("type", toml_edit::value("fastembed"));
    provider.insert("model", toml_edit::value(model));

    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("Failed to create {}", parent.display()))?;
    }
    std::fs::write(path, document.to_string())
        .with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(previous)
}

/// The refusal for a name the catalog does not hold, with the near names.
fn unknown_model(name: &str, catalog: &EmbeddingCatalog) -> String {
    let wanted = name.trim().to_ascii_lowercase();
    let mut near: Vec<&str> = catalog
        .models
        .iter()
        .filter(|m| m.name.to_ascii_lowercase().contains(&wanted))
        .map(|m| m.name.as_str())
        .collect();
    if near.is_empty() {
        near = catalog
            .models
            .iter()
            .filter(|m| m.recommended)
            .map(|m| m.name.as_str())
            .collect();
    }
    near.truncate(4);
    format!(
        "Unknown embedding model '{name}'. Try one of: {}. Run `cru models embeddings` for the \
         whole catalog.",
        near.join(", ")
    )
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

        let previous = write_model(&path, "arctic-embed-m").expect("write");
        assert_eq!(previous.as_deref(), Some("bge-small-en-v1.5"));

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

        let previous = write_model(&path, "bge-base-en-v1.5").expect("write");
        assert_eq!(previous, None);
        let written = std::fs::read_to_string(&path).expect("read back");
        assert!(
            written.contains("model = \"bge-base-en-v1.5\""),
            "{written}"
        );
    }
}
