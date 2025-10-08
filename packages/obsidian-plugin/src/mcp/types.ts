/**
 * TypeScript type definitions for MCP (Model Context Protocol)
 * Mirrors the Rust types defined in crates/crucible-mcp/src/types.rs
 */

/**
 * JSON-RPC 2.0 request structure
 */
export interface JsonRpcRequest {
  jsonrpc: string;
  id?: number | string | null;
  method: string;
  params?: any;
}

/**
 * JSON-RPC 2.0 response structure
 */
export interface JsonRpcResponse {
  jsonrpc: string;
  id?: number | string | null;
  result?: any;
  error?: JsonRpcError;
}

/**
 * JSON-RPC 2.0 notification (no response expected)
 */
export interface JsonRpcNotification {
  jsonrpc: string;
  method: string;
  params?: any;
}

/**
 * JSON-RPC error object
 */
export interface JsonRpcError {
  code: number;
  message: string;
  data?: any;
}

/**
 * MCP server capabilities
 */
export interface ServerCapabilities {
  tools?: ToolsCapability;
}

/**
 * Tools capability structure
 */
export interface ToolsCapability {
  listChanged?: boolean;
}

/**
 * Client capabilities
 */
export interface ClientCapabilities {
  experimental?: any;
}

/**
 * Client information
 */
export interface ClientInfo {
  name: string;
  version: string;
}

/**
 * Server information
 */
export interface ServerInfo {
  name: string;
  version: string;
}

/**
 * MCP initialization request parameters
 */
export interface InitializeRequest {
  protocolVersion: string;
  capabilities: ClientCapabilities;
  clientInfo: ClientInfo;
}

/**
 * MCP initialization response
 */
export interface InitializeResponse {
  protocolVersion: string;
  capabilities: ServerCapabilities;
  serverInfo: ServerInfo;
}

/**
 * MCP tool definition
 */
export interface McpTool {
  name: string;
  description: string;
  inputSchema: any; // JSON Schema
}

/**
 * List tools response
 */
export interface ListToolsResponse {
  tools: McpTool[];
}

/**
 * Tool call request parameters
 */
export interface CallToolRequest {
  name: string;
  arguments: any;
}

/**
 * MCP content types
 */
export type McpContent =
  | { type: "text"; text: string }
  | { type: "image"; data: string; mimeType: string }
  | { type: "resource"; resource: ResourceContent };

/**
 * Resource content
 */
export interface ResourceContent {
  uri: string;
  mimeType?: string;
  text?: string;
}

/**
 * Tool call response
 */
export interface CallToolResponse {
  content: McpContent[];
  isError?: boolean;
}

/**
 * MCP client configuration
 */
export interface McpClientConfig {
  /** Path to the MCP server executable */
  serverPath: string;
  /** Optional arguments to pass to the server */
  serverArgs?: string[];
  /** Client name for identification */
  clientName: string;
  /** Client version */
  clientVersion: string;
  /** Timeout for requests in milliseconds (default: 30000) */
  requestTimeout?: number;
  /** Enable debug logging */
  debug?: boolean;
}

/**
 * MCP client events
 */
export interface McpClientEvents {
  /** Server process started */
  started: () => void;
  /** Server initialized successfully */
  initialized: (response: InitializeResponse) => void;
  /** Server process stopped */
  stopped: () => void;
  /** Error occurred */
  error: (error: Error) => void;
  /** Server process exited */
  exit: (code: number | null, signal: string | null) => void;
}

/**
 * Standard JSON-RPC error codes
 */
export enum JsonRpcErrorCode {
  ParseError = -32700,
  InvalidRequest = -32600,
  MethodNotFound = -32601,
  InvalidParams = -32602,
  InternalError = -32603,
  ServerNotInitialized = -32002,
}
