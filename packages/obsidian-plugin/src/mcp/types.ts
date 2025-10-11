/**
 * TypeScript type definitions for MCP (Model Context Protocol)
 *
 * This file provides type compatibility between our custom types and the
 * official MCP SDK types. We maintain our existing API for backward compatibility
 * while leveraging the official SDK implementation.
 */

// Import MCP SDK types for internal use (avoid re-export conflicts)
import type {
  ClientCapabilities as McpSdkClientCapabilities,
  ServerCapabilities as McpSdkServerCapabilities,
  Tool as McpSdkTool,
  CallToolRequest as McpSdkCallToolRequest,
  CallToolResult as McpSdkCallToolResult,
  TextContent,
  ImageContent,
  EmbeddedResource
} from "@modelcontextprotocol/sdk/types.js";

/**
 * JSON-RPC 2.0 request structure (legacy - for backward compatibility)
 */
export interface JsonRpcRequest {
  jsonrpc: string;
  id?: number | string | null;
  method: string;
  params?: any;
}

/**
 * JSON-RPC 2.0 response structure (legacy - for backward compatibility)
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
 * MCP initialization request parameters (legacy format)
 */
export interface InitializeRequest {
  protocol_version: string;
  capabilities: ClientCapabilities;
  client_info: ClientInfo;
}

/**
 * MCP initialization response (our legacy format)
 */
export interface InitializeResponse {
  protocol_version: string;
  capabilities: ServerCapabilities;
  server_info: ServerInfo;
}

/**
 * MCP tool definition (our legacy format for compatibility)
 */
export interface McpTool {
  name: string;
  description: string;
  inputSchema: any; // JSON Schema
}

/**
 * List tools response (legacy format)
 */
export interface ListToolsResponse {
  tools: McpTool[];
}

/**
 * Tool call request parameters (legacy format)
 */
export interface CallToolRequest {
  name: string;
  arguments: any;
}

/**
 * MCP content types (our legacy format for compatibility)
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
 * Tool call response (our legacy format for compatibility)
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

/**
 * Utility functions for converting between MCP SDK and legacy formats
 */
export class McpTypeConverter {
  /**
   * Convert MCP SDK tool to our legacy McpTool format
   */
  static fromSdkTool(sdkTool: any): McpTool {
    return {
      name: sdkTool.name,
      description: sdkTool.description,
      inputSchema: sdkTool.inputSchema,
    };
  }

  /**
   * Convert our legacy McpTool to MCP SDK format
   */
  static toSdkTool(legacyTool: McpTool): any {
    return {
      name: legacyTool.name,
      description: legacyTool.description,
      inputSchema: legacyTool.inputSchema,
    };
  }

  /**
   * Convert MCP SDK CallToolResult to our legacy CallToolResponse format
   */
  static fromSdkCallResult(sdkResult: any): CallToolResponse {
    return {
      content: sdkResult.content || [],
      isError: sdkResult.isError || false,
    };
  }

  /**
   * Convert MCP SDK content to our legacy McpContent format
   */
  static fromSdkContent(sdkContent: any): McpContent {
    if (sdkContent.type === "text") {
      return {
        type: "text",
        text: sdkContent.text,
      };
    } else if (sdkContent.type === "image") {
      return {
        type: "image",
        data: sdkContent.data,
        mimeType: sdkContent.mimeType,
      };
    } else if (sdkContent.type === "resource") {
      return {
        type: "resource",
        resource: sdkContent.resource,
      };
    }

    // Fallback for unknown types
    return {
      type: "text",
      text: JSON.stringify(sdkContent),
    };
  }
}