use crucible_core::DocumentNode;
use crucible_mcp::McpServer;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::collections::HashMap;

// Global MCP server instance
static mut MCP_SERVER: Option<McpServer> = None;

async fn get_mcp_server() -> std::result::Result<&'static McpServer, String> {
    unsafe {
        if MCP_SERVER.is_none() {
            let db_path = "crucible.db";
            MCP_SERVER = Some(McpServer::new(db_path).await.map_err(|e| e.to_string())?);
        }
        Ok(MCP_SERVER.as_ref().unwrap())
    }
}

#[derive(Debug, Serialize, Deserialize)]
pub struct CreateDocumentRequest {
    pub title: String,
    pub content: String,
}

#[tauri::command]
pub async fn greet(name: &str) -> std::result::Result<String, String> {
    Ok(format!("Hello, {}! You've been greeted from Rust!", name))
}

#[tauri::command]
pub async fn initialize_database() -> std::result::Result<(), String> {
    // Initialize the MCP server if not already done
    get_mcp_server().await?;
    Ok(())
}

#[tauri::command]
pub async fn search_documents(query: String) -> std::result::Result<Vec<serde_json::Value>, String> {
    let server = get_mcp_server().await?;
    let args = json!({ "query": query });
    let result = server.handle_tool_call("search_by_content", args).await.map_err(|e| e.to_string())?;
    
    if result.success {
        Ok(result.data.unwrap().as_array().unwrap().clone())
    } else {
        Err(result.error.unwrap_or("Search failed".to_string()))
    }
}

#[tauri::command]
pub async fn get_document(path: String) -> std::result::Result<serde_json::Value, String> {
    let server = get_mcp_server().await?;
    let args = json!({ "path": path });
    let result = server.handle_tool_call("get_note_metadata", args).await.map_err(|e| e.to_string())?;
    
    if result.success {
        Ok(result.data.unwrap())
    } else {
        Err(result.error.unwrap_or("Failed to get document".to_string()))
    }
}

#[tauri::command]
pub async fn create_document(
    request: CreateDocumentRequest,
) -> std::result::Result<DocumentNode, String> {
    let document = DocumentNode::new(request.title.clone(), request.content.clone());
    
    // Store in database via MCP
    let server = get_mcp_server().await?;
    let args = json!({
        "path": format!("{}.md", request.title),
        "properties": {
            "title": request.title,
            "created": true
        }
    });
    let _result = server.handle_tool_call("update_note_properties", args).await.map_err(|e| e.to_string())?;
    
    Ok(document)
}

#[tauri::command]
pub async fn update_document(path: String, content: String) -> std::result::Result<(), String> {
    let server = get_mcp_server().await?;
    let args = json!({
        "path": path,
        "properties": {
            "updated": true,
            "last_modified": chrono::Utc::now().to_rfc3339()
        }
    });
    let result = server.handle_tool_call("update_note_properties", args).await.map_err(|e| e.to_string())?;
    
    if result.success {
        Ok(())
    } else {
        Err(result.error.unwrap_or("Failed to update document".to_string()))
    }
}

#[tauri::command]
pub async fn delete_document(path: String) -> std::result::Result<(), String> {
    // TODO: Implement document deletion
    Err("Not implemented".to_string())
}

#[tauri::command]
pub async fn list_documents() -> std::result::Result<Vec<String>, String> {
    let server = get_mcp_server().await?;
    let result = server.handle_tool_call("search_by_filename", json!({"pattern": "*.md"})).await.map_err(|e| e.to_string())?;
    
    if result.success {
        Ok(result.data.unwrap().as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect())
    } else {
        Err(result.error.unwrap_or("Failed to list documents".to_string()))
    }
}

#[tauri::command]
pub async fn search_by_tags(tags: Vec<String>) -> std::result::Result<Vec<String>, String> {
    let server = get_mcp_server().await?;
    let args = json!({ "tags": tags });
    let result = server.handle_tool_call("search_by_tags", args).await.map_err(|e| e.to_string())?;
    
    if result.success {
        Ok(result.data.unwrap().as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect())
    } else {
        Err(result.error.unwrap_or("Failed to search by tags".to_string()))
    }
}

#[tauri::command]
pub async fn search_by_properties(properties: HashMap<String, serde_json::Value>) -> std::result::Result<Vec<String>, String> {
    let server = get_mcp_server().await?;
    let args = json!({ "properties": properties });
    let result = server.handle_tool_call("search_by_properties", args).await.map_err(|e| e.to_string())?;
    
    if result.success {
        Ok(result.data.unwrap().as_array().unwrap().iter().map(|v| v.as_str().unwrap().to_string()).collect())
    } else {
        Err(result.error.unwrap_or("Failed to search by properties".to_string()))
    }
}

#[tauri::command]
pub async fn semantic_search(query: String, top_k: u32) -> std::result::Result<Vec<serde_json::Value>, String> {
    let server = get_mcp_server().await?;
    let args = json!({ "query": query, "top_k": top_k });
    let result = server.handle_tool_call("semantic_search", args).await.map_err(|e| e.to_string())?;
    
    if result.success {
        Ok(result.data.unwrap().as_array().unwrap().clone())
    } else {
        Err(result.error.unwrap_or("Failed to perform semantic search".to_string()))
    }
}

#[tauri::command]
pub async fn index_vault(force: bool) -> std::result::Result<serde_json::Value, String> {
    let server = get_mcp_server().await?;
    let args = json!({ "force": force });
    let result = server.handle_tool_call("index_vault", args).await.map_err(|e| e.to_string())?;
    
    if result.success {
        Ok(result.data.unwrap())
    } else {
        Err(result.error.unwrap_or("Failed to index vault".to_string()))
    }
}

#[tauri::command]
pub async fn get_note_metadata(path: String) -> std::result::Result<serde_json::Value, String> {
    let server = get_mcp_server().await?;
    let args = json!({ "path": path });
    let result = server.handle_tool_call("get_note_metadata", args).await.map_err(|e| e.to_string())?;
    
    if result.success {
        Ok(result.data.unwrap())
    } else {
        Err(result.error.unwrap_or("Failed to get note metadata".to_string()))
    }
}

#[tauri::command]
pub async fn update_note_properties(path: String, properties: HashMap<String, serde_json::Value>) -> std::result::Result<(), String> {
    let server = get_mcp_server().await?;
    let args = json!({ "path": path, "properties": properties });
    let result = server.handle_tool_call("update_note_properties", args).await.map_err(|e| e.to_string())?;
    
    if result.success {
        Ok(())
    } else {
        Err(result.error.unwrap_or("Failed to update note properties".to_string()))
    }
}