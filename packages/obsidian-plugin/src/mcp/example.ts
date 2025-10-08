/**
 * Example usage of the MCP Client
 *
 * This file demonstrates how to use the McpClient to communicate
 * with the Rust MCP server.
 */

import { McpClient } from "./client";
import type { McpTool, CallToolResponse } from "./types";

/**
 * Example 1: Basic client usage
 */
async function basicExample() {
  // Create the client
  const client = new McpClient({
    serverPath: "/path/to/crucible-mcp", // Update with actual path
    serverArgs: ["--db-path", "/path/to/vault.db"],
    clientName: "example-client",
    clientVersion: "1.0.0",
    debug: true,
  });

  try {
    // Start the server and initialize
    const initResponse = await client.start();
    console.log("Connected to:", initResponse.server_info);

    // List available tools
    const tools = await client.listTools();
    console.log("Available tools:");
    tools.forEach((tool) => {
      console.log(`  - ${tool.name}: ${tool.description}`);
    });

    // Call a tool
    const result = await client.callTool("search_files", {
      query: "typescript",
      top_k: 10,
    });
    console.log("Search results:", result);

    // Clean up
    await client.stop();
  } catch (error) {
    console.error("Error:", error);
    await client.stop();
  }
}

/**
 * Example 2: Event-driven usage
 */
async function eventDrivenExample() {
  const client = new McpClient({
    serverPath: "/path/to/crucible-mcp",
    clientName: "event-client",
    clientVersion: "1.0.0",
  });

  // Set up event listeners
  client.on("started", () => {
    console.log("Server process started");
  });

  client.on("initialized", (response) => {
    console.log("Initialized:", response.server_info.name);
  });

  client.on("error", (error) => {
    console.error("MCP Error:", error.message);
  });

  client.on("exit", (code, signal) => {
    console.log("Server exited:", { code, signal });
  });

  client.on("stopped", () => {
    console.log("Client stopped");
  });

  // Start the client
  await client.start();

  // Do some work
  const tools = await client.listTools();
  console.log(`Found ${tools.length} tools`);

  // Clean up
  await client.stop();
}

/**
 * Example 3: Error handling
 */
async function errorHandlingExample() {
  const client = new McpClient({
    serverPath: "/path/to/crucible-mcp",
    clientName: "error-client",
    clientVersion: "1.0.0",
    requestTimeout: 5000, // 5 second timeout
  });

  try {
    await client.start();

    // Handle JSON-RPC errors
    try {
      await client.callTool("non_existent_tool", {});
    } catch (error: any) {
      if (error.code === -32601) {
        console.error("Tool not found");
      }
    }

    // Handle timeout errors
    try {
      await client.callTool("slow_tool", {});
    } catch (error: any) {
      if (error.message.includes("timeout")) {
        console.error("Request timed out");
      }
    }

    await client.stop();
  } catch (error) {
    console.error("Failed to start client:", error);
  }
}

/**
 * Example 4: Checking client status
 */
async function statusCheckExample() {
  const client = new McpClient({
    serverPath: "/path/to/crucible-mcp",
    clientName: "status-client",
    clientVersion: "1.0.0",
  });

  console.log("Is ready?", client.isReady()); // false

  await client.start();

  console.log("Is ready?", client.isReady()); // true

  const serverInfo = client.getServerInfo();
  if (serverInfo) {
    console.log("Server name:", serverInfo.server_info.name);
    console.log("Server version:", serverInfo.server_info.version);
    console.log("Protocol version:", serverInfo.protocol_version);
  }

  await client.stop();

  console.log("Is ready?", client.isReady()); // false
}

// Run examples (uncomment to test)
// basicExample();
// eventDrivenExample();
// errorHandlingExample();
// statusCheckExample();
