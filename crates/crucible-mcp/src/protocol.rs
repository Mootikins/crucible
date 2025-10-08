// crates/crucible-mcp/src/protocol.rs
use crate::types::*;
use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::io::{self, BufRead, BufReader, Write};
use tracing::{debug, error, info, warn};

/// JSON-RPC 2.0 request structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcRequest {
    pub jsonrpc: String,
    pub id: Option<Value>,
    pub method: String,
    pub params: Option<Value>,
}

/// JSON-RPC 2.0 response structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcResponse {
    pub jsonrpc: String,
    pub id: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<JsonRpcError>,
}

/// JSON-RPC notification (no response expected)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct JsonRpcNotification {
    pub jsonrpc: String,
    pub method: String,
    pub params: Option<Value>,
}

/// MCP Protocol handler
pub struct McpProtocolHandler {
    server_name: String,
    server_version: String,
    initialized: bool,
}

impl McpProtocolHandler {
    pub fn new(server_name: String, server_version: String) -> Self {
        Self {
            server_name,
            server_version,
            initialized: false,
        }
    }

    /// Handle incoming JSON-RPC message
    pub async fn handle_message(&mut self, message: &str) -> Result<Option<String>> {
        debug!("Received message: {}", message);

        // Try to parse as request first
        if let Ok(request) = serde_json::from_str::<JsonRpcRequest>(message) {
            return self.handle_request(request).await;
        }

        // Try to parse as notification
        if let Ok(notification) = serde_json::from_str::<JsonRpcNotification>(message) {
            self.handle_notification(notification).await;
            return Ok(None); // Notifications don't require responses
        }

        Err(anyhow!("Invalid JSON-RPC message format"))
    }

    /// Handle JSON-RPC request
    async fn handle_request(&mut self, request: JsonRpcRequest) -> Result<Option<String>> {
        let response = match request.method.as_str() {
            "initialize" => self.handle_initialize(request.id, request.params).await?,
            "tools/list" => self.handle_list_tools(request.id).await?,
            "tools/call" => self.handle_call_tool(request.id, request.params).await?,
            _ => {
                // Unknown method
                JsonRpcResponse {
                    jsonrpc: "2.0".to_string(),
                    id: request.id,
                    result: None,
                    error: Some(JsonRpcError {
                        code: -32601,
                        message: "Method not found".to_string(),
                        data: Some(json!({ "method": request.method })),
                    }),
                }
            }
        };

        Ok(Some(serde_json::to_string(&response)?))
    }

    /// Handle JSON-RPC notification
    async fn handle_notification(&mut self, notification: JsonRpcNotification) {
        match notification.method.as_str() {
            "initialized" => {
                info!("Client confirmed initialization");
                self.initialized = true;
            }
            "notifications/cancelled" => {
                debug!("Request cancelled: {:?}", notification.params);
            }
            _ => {
                warn!("Unknown notification method: {}", notification.method);
            }
        }
    }

    /// Handle initialize request
    async fn handle_initialize(
        &mut self,
        id: Option<Value>,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse> {
        let _init_request: InitializeRequest = if let Some(params) = params {
            serde_json::from_value(params)?
        } else {
            return Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Invalid params".to_string(),
                    data: None,
                }),
            });
        };

        let response = InitializeResponse {
            protocol_version: "2024-11-05".to_string(),
            capabilities: ServerCapabilities {
                tools: Some(ToolsCapability {
                    list_changed: Some(false),
                }),
            },
            server_info: ServerInfo {
                name: self.server_name.clone(),
                version: self.server_version.clone(),
            },
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(response)?),
            error: None,
        })
    }

    /// Handle list tools request
    async fn handle_list_tools(&self, id: Option<Value>) -> Result<JsonRpcResponse> {
        if !self.initialized {
            return Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32002,
                    message: "Server not initialized".to_string(),
                    data: None,
                }),
            });
        }

        let tools = crate::McpServer::get_tools();
        let response = ListToolsResponse { tools };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(response)?),
            error: None,
        })
    }

    /// Handle call tool request
    async fn handle_call_tool(
        &self,
        id: Option<Value>,
        params: Option<Value>,
    ) -> Result<JsonRpcResponse> {
        if !self.initialized {
            return Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32002,
                    message: "Server not initialized".to_string(),
                    data: None,
                }),
            });
        }

        let _call_request: CallToolRequest = if let Some(params) = params {
            serde_json::from_value(params)?
        } else {
            return Ok(JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Invalid params".to_string(),
                    data: None,
                }),
            });
        };

        // This would need access to the McpServer instance
        // For now, return a placeholder response
        let response = CallToolResponse {
            content: vec![McpContent::Text {
                text: "Tool execution not implemented in protocol handler".to_string(),
            }],
            is_error: Some(true),
        };

        Ok(JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id,
            result: Some(serde_json::to_value(response)?),
            error: None,
        })
    }
}

/// STDIO-based MCP server
pub struct StdioMcpServer {
    protocol_handler: McpProtocolHandler,
    mcp_server: Option<crate::McpServer>,
}

impl StdioMcpServer {
    pub fn new(server_name: String, server_version: String) -> Self {
        Self {
            protocol_handler: McpProtocolHandler::new(server_name, server_version),
            mcp_server: None,
        }
    }

    /// Initialize with MCP server instance
    pub async fn initialize(&mut self, db_path: &str) -> Result<()> {
        self.mcp_server = Some(crate::McpServer::new(db_path).await?);
        Ok(())
    }

    /// Run the server over stdio
    pub async fn run_stdio(&mut self) -> Result<()> {
        info!("Starting MCP server over stdio");

        let stdin = io::stdin();
        let mut stdout = io::stdout();

        let reader = BufReader::new(stdin);
        let mut lines = reader.lines();

        while let Some(line) = lines.next() {
            let line = line?;
            if line.trim().is_empty() {
                continue;
            }

            match self.handle_line(&line).await {
                Ok(Some(response)) => {
                    writeln!(stdout, "{}", response)?;
                    stdout.flush()?;
                }
                Ok(None) => {
                    // No response needed (notification)
                }
                Err(e) => {
                    error!("Error handling message: {}", e);
                    let error_response = JsonRpcResponse {
                        jsonrpc: "2.0".to_string(),
                        id: None,
                        result: None,
                        error: Some(JsonRpcError {
                            code: -32603,
                            message: "Internal error".to_string(),
                            data: Some(json!({ "details": e.to_string() })),
                        }),
                    };
                    writeln!(stdout, "{}", serde_json::to_string(&error_response)?)?;
                    stdout.flush()?;
                }
            }
        }

        Ok(())
    }

    /// Handle a single line of input
    async fn handle_line(&mut self, line: &str) -> Result<Option<String>> {
        match self.protocol_handler.handle_message(line).await {
            Ok(response) => {
                // If it's a tool call and we have a server, handle it properly
                if line.contains("tools/call") && self.mcp_server.is_some() {
                    return self.handle_tool_call_with_server(line).await;
                }
                Ok(response)
            }
            Err(e) => Err(e),
        }
    }

    /// Handle tool call with actual server instance
    async fn handle_tool_call_with_server(&mut self, line: &str) -> Result<Option<String>> {
        let request: JsonRpcRequest = serde_json::from_str(line)?;
        let call_request: CallToolRequest = if let Some(params) = request.params {
            serde_json::from_value(params)?
        } else {
            return Ok(Some(serde_json::to_string(&JsonRpcResponse {
                jsonrpc: "2.0".to_string(),
                id: request.id,
                result: None,
                error: Some(JsonRpcError {
                    code: -32602,
                    message: "Invalid params".to_string(),
                    data: None,
                }),
            })?));
        };

        let server = self.mcp_server.as_ref().unwrap();
        let tool_result = server
            .handle_tool_call(&call_request.name, call_request.arguments)
            .await?;

        let content = if tool_result.success {
            if let Some(data) = tool_result.data {
                vec![McpContent::Text {
                    text: serde_json::to_string_pretty(&data)?,
                }]
            } else {
                vec![McpContent::Text {
                    text: "Success".to_string(),
                }]
            }
        } else {
            vec![McpContent::Text {
                text: tool_result.error.unwrap_or("Unknown error".to_string()),
            }]
        };

        let response = CallToolResponse {
            content,
            is_error: Some(!tool_result.success),
        };

        let json_response = JsonRpcResponse {
            jsonrpc: "2.0".to_string(),
            id: request.id,
            result: Some(serde_json::to_value(response)?),
            error: None,
        };

        Ok(Some(serde_json::to_string(&json_response)?))
    }
}
