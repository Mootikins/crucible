use crate::cli::NoteCommands;
use crate::config::CliConfig;
use crate::output;
use anyhow::Result;
use async_trait::async_trait;
use serde_json::json;
use std::sync::Arc;

#[async_trait]
pub trait ToolInvoker: Send + Sync {
    async fn ensure_initialized(&self) -> Result<()>;
    async fn execute(
        &self,
        name: &str,
        payload: serde_json::Value,
        user_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<crucible_tools::ToolResult>;
}

#[derive(Default, Clone)]
struct GlobalToolInvoker;

#[async_trait]
impl ToolInvoker for GlobalToolInvoker {
    async fn ensure_initialized(&self) -> Result<()> {
        crate::common::CrucibleToolManager::ensure_initialized_global().await
    }

    async fn execute(
        &self,
        name: &str,
        payload: serde_json::Value,
        user_id: Option<&str>,
        session_id: Option<&str>,
    ) -> Result<crucible_tools::ToolResult> {
        crate::common::CrucibleToolManager::execute_tool_global(
            name,
            payload,
            user_id.map(|s| s.to_string()),
            session_id.map(|s| s.to_string()),
        )
        .await
    }
}

pub async fn execute(config: CliConfig, cmd: NoteCommands) -> Result<()> {
    let invoker: Arc<dyn ToolInvoker> = Arc::new(GlobalToolInvoker::default());

    match cmd {
        NoteCommands::Get { path, format } => get_note(invoker, config, path, format).await,
        NoteCommands::Create {
            path,
            content,
            edit,
        } => create_note(config, path, content, edit).await,
        NoteCommands::Update { path, properties } => update_note(config, path, properties).await,
        NoteCommands::List { format } => list_notes(invoker, config, format).await,
    }
}

async fn get_note(
    invoker: Arc<dyn ToolInvoker>,
    config: CliConfig,
    path: String,
    format: String,
) -> Result<()> {
    // Use search_by_content tool to find the note
    let result = invoker
        .execute(
            "search_by_content",
            json!({
                "query": path,
                "limit": 1
            }),
            Some("cli_user"),
            Some("note_session"),
        )
        .await?;

    if let Some(data) = result.data {
        if let Some(results) = data.get("results").and_then(|r| r.as_array()) {
            if let Some(first_result) = results.first() {
                match format.as_str() {
                    "json" => println!("{}", serde_json::to_string_pretty(first_result)?),
                    _ => {
                        println!("📄 Path: {}", path);
                        if let Some(metadata) = first_result.get("metadata") {
                            if let Some(title) = metadata.get("title").and_then(|t| t.as_str()) {
                                println!("📝 Title: {}", title);
                            }
                        }
                        if let Some(content) = first_result.get("content").and_then(|c| c.as_str())
                        {
                            println!("\n{}", content);
                        }
                    }
                }
                return Ok(());
            }
        }
    }

    // If not found via search, try reading directly from file system
    let full_path = config.kiln.path.join(&path);
    if full_path.exists() {
        let content = std::fs::read_to_string(&full_path)?;
        match format.as_str() {
            "json" => {
                let note_data = json!({
                    "path": path,
                    "content": content,
                    "source": "file_system"
                });
                println!("{}", serde_json::to_string_pretty(&note_data)?);
            }
            _ => {
                println!("📄 Path: {}", path);
                println!("\n{}", content);
            }
        }
    } else {
        anyhow::bail!("Note not found: {}", path);
    }

    Ok(())
}

async fn create_note(
    config: CliConfig,
    path: String,
    content: Option<String>,
    edit: bool,
) -> Result<()> {
    let full_path = config.kiln.path.join(&path);

    // Create parent directories
    if let Some(parent) = full_path.parent() {
        std::fs::create_dir_all(parent)?;
    }

    // Write content
    let content = content.unwrap_or_else(|| String::from("# New Note\n\n"));
    std::fs::write(&full_path, &content)?;

    println!("Created: {}", full_path.display());

    // Open in editor if requested
    if edit {
        let editor = std::env::var("EDITOR").unwrap_or_else(|_| "vim".to_string());
        std::process::Command::new(editor)
            .arg(&full_path)
            .status()?;
    }

    Ok(())
}

async fn update_note(_config: CliConfig, path: String, properties: String) -> Result<()> {
    let _props: serde_json::Value = serde_json::from_str(&properties)?;

    // TODO: Implement property updates in frontmatter
    println!("Note: Property updates not yet implemented");
    println!("This will update frontmatter for: {}", path);

    Ok(())
}

async fn list_notes(
    invoker: Arc<dyn ToolInvoker>,
    _config: CliConfig,
    format: String,
) -> Result<()> {
    // Use simplified tools approach instead of direct database access
    invoker.ensure_initialized().await?;

    // Get all documents using search_by_folder tool
    let result = invoker
        .execute(
            "search_by_folder",
            serde_json::json!({
                "path": ".",
                "recursive": true
            }),
            Some("cli_user"),
            Some("list_notes"),
        )
        .await?;

    let files: Vec<String> = if let Some(data) = result.data {
        data.get("files")
            .and_then(|f| f.as_array())
            .map(|files| {
                files
                    .iter()
                    .filter_map(|item| item.get("path").and_then(|p| p.as_str()))
                    .map(|s| s.to_string())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };

    let output = output::format_file_list(&files, &format)?;
    println!("{}", output);

    Ok(())
}
