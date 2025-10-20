use anyhow::{Context, Result};
use crucible_core::database::{Database, DocumentId};
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
    let db = Database::new(&config.database_path_str()?).await?;

    let doc = db.get_document(&DocumentId(path.clone())).await?
        .context(format!("Note not found: {}", path))?;

    match format.as_str() {
        "json" => println!("{}", serde_json::to_string_pretty(&doc)?),
        _ => {
            println!("Path: {}", path);
            if let Some(title) = doc.title {
                println!("Title: {}", title);
            }
            if !doc.folder.is_empty() {
                println!("Folder: {}", doc.folder);
            }
            println!("\n{}", doc.content);
        }
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
    let db = Database::new(&config.database_path_str()?).await?;

    // Get all documents
    let search_options = crucible_core::database::SearchOptions {
        limit: Some(10000), // Large limit to get all documents
        offset: Some(0),
        filters: None,
    };

    let search_results = db.search("", search_options).await?;
    let files: Vec<String> = search_results
        .into_iter()
        .map(|result| result.document_id.0)
        .collect();

    let output = output::format_file_list(&files, &format)?;
    println!("{}", output);

    Ok(())
}
