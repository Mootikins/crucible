use anyhow::{Context, Result};
use crucible_mcp::database::EmbeddingDatabase;
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
    let db = EmbeddingDatabase::new(&config.database_path_str()?).await?;
    
    let data = db.get_embedding(&path).await?
        .context(format!("Note not found: {}", path))?;
    
    match format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&data)?),
        _ => {
            println!("Path: {}", data.file_path);
            println!("\n{}", data.content);
        }
    }
    
    Ok(())
}

async fn create_note(config: CliConfig, path: String, content: Option<String>, edit: bool) -> Result<()> {
    use std::io::Write;

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
    let db = EmbeddingDatabase::new(&config.database_path_str()?).await?;
    let files = db.list_files().await?;
    
    let output = output::format_file_list(&files, &format)?;
    println!("{}", output);
    
    Ok(())
}
