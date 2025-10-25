use anyhow::Result;
use crate::common::CrucibleToolManager;
use serde_json::json;
use crate::config::CliConfig;
use crate::cli::NoteCommands;
use crate::output;

pub async fn execute(config: CliConfig, cmd: NoteCommands) -> Result<()> {
    
    match cmd {
        NoteCommands::Get { path, format } => get_note(config, path, format).await,
        NoteCommands::Create { path, content, edit } => create_note(config, path, content, edit).await,
        NoteCommands::Update { path, properties } => update_note(config, path, properties).await,
        NoteCommands::List { format } => list_notes(config, format).await,
    }
}

async fn get_note(config: CliConfig, path: String, format: String) -> Result<()> {
    // Use search_by_content tool to find the note
    let result = CrucibleToolManager::execute_tool_global(
        "search_by_content",
        json!({
            "query": path,
            "limit": 1
        }),
        Some("cli_user".to_string()),
        Some("note_session".to_string()),
    ).await?;

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
                        if let Some(content) = first_result.get("content").and_then(|c| c.as_str()) {
                            println!("\n{}", content);
                        }
                    }
                }
                return Ok(());
            }
        }
    }

    // If not found via search, try reading directly from file system
    let full_path = config.vault.path.join(&path);
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

async fn create_note(config: CliConfig, path: String, content: Option<String>, edit: bool) -> Result<()> {
    

    let full_path = config.vault.path.join(&path);
    
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

async fn update_note(config: CliConfig, path: String, properties: String) -> Result<()> {
    let _props: serde_json::Value = serde_json::from_str(&properties)?;
    
    // TODO: Implement property updates in frontmatter
    println!("Note: Property updates not yet implemented");
    println!("This will update frontmatter for: {}", path);
    
    Ok(())
}

async fn list_notes(config: CliConfig, format: String) -> Result<()> {
    // Use simplified tools approach instead of direct database access
    CrucibleToolManager::ensure_initialized_global().await?;

    // Get all documents using search_by_folder tool
    let result = CrucibleToolManager::execute_tool_global(
        "search_by_folder",
        serde_json::json!({
            "path": ".",
            "recursive": true
        }),
        Some("cli_user".to_string()),
        Some("list_notes".to_string()),
    ).await?;

    let files: Vec<String> = if let Some(data) = result.data {
        data.get("files")
            .and_then(|f| f.as_array())
            .map(|files| {
                files.iter()
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
