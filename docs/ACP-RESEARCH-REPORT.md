# Agent Client Protocol (ACP) Research Report

**Research Date:** 2025-11-08
**Protocol Version:** v0.6.3 (latest as of October 2025)
**Primary Source:** https://github.com/agentclientprotocol/agent-client-protocol

---

## Executive Summary

### Key Findings

1. **ACP is a subprocess-based protocol** using JSON-RPC 2.0 over stdin/stdout, similar to LSP
2. **It is NOT a server-client protocol** - agents run as subprocesses of the editor/IDE
3. **It complements MCP** - ACP handles editor-agent communication, MCP handles tool/data access
4. **Embedded library pattern** - Applications embed the `agent-client-protocol` Rust crate directly
5. **Protocol sits between UI and Agent** - The editor acts as the client, the AI agent as the server

### Protocol Purpose

The Agent Client Protocol (ACP) standardizes communication between code editors/IDEs and AI-powered coding agents. It solves the fragmentation problem where each editor-agent combination previously required custom integration work.

**Problem Solved:** Before ACP, integrating an AI agent (like Claude Code, Gemini CLI, or Codex) with an editor (like Zed, VSCode, or JetBrains) required bespoke development for each pairing. ACP provides a universal standard, similar to how LSP standardized language server integration.

---

## 1. Protocol Overview

### What is ACP?

**Definition:** ACP is an open standard that enables any AI coding agent to integrate with any code editor through a standardized JSON-RPC 2.0 communication protocol.

**Core Behaviors Specified:**
- Initialize: Capability and version negotiation
- Authenticate: Optional credential validation
- Session Management: Create/load conversation contexts
- Prompt: Send user input to agent
- Stream Progress: Real-time status updates
- Request Permission: User authorization for sensitive operations
- File Operations: Optional read/write primitives

### How It Differs from MCP

| Aspect | MCP (Model Context Protocol) | ACP (Agent Client Protocol) |
|--------|------------------------------|------------------------------|
| **Purpose** | Connects LLMs to external tools/data | Connects editors to AI agents |
| **Scope** | Tool invocation, resource access | Editor-agent communication |
| **Focus** | "What" - data and capabilities | "Where" - workflow integration |
| **Architecture** | Client-server, multiple transports | Subprocess, stdio-based |
| **Integration** | LLM accesses external systems | Editor launches and controls agent |
| **Relationship** | Used BY agents for tool access | Used BY editors for agent control |

**Key Insight:** ACP and MCP work together. When an ACP session starts, the editor passes available MCP server endpoints to the agent, enabling the agent to invoke tools through MCP while the editor maintains oversight through ACP.

---

## 2. Architecture

### Communication Model

```
┌─────────────────────────────────────────────────────────┐
│                    Code Editor/IDE                       │
│                                                          │
│  ┌────────────────────────────────────────────────┐    │
│  │          ClientSideConnection                   │    │
│  │  (implements Client trait)                      │    │
│  │                                                  │    │
│  │  - UI events → session/prompt                   │    │
│  │  - Handle session/update notifications          │    │
│  │  - Present permission requests to user          │    │
│  │  - Provide file system access (opt-in)          │    │
│  └────────────────────────────────────────────────┘    │
│                          │                              │
│                          │ JSON-RPC 2.0                 │
│                          │ over stdin/stdout            │
└──────────────────────────┼──────────────────────────────┘
                           │
                           │ subprocess
                           │
┌──────────────────────────┼──────────────────────────────┐
│                          ▼                              │
│  ┌────────────────────────────────────────────────┐    │
│  │          AgentSideConnection                    │    │
│  │  (implements Agent trait)                       │    │
│  │                                                  │    │
│  │  - Receive prompts                              │    │
│  │  - Stream updates (tool calls, thoughts)        │    │
│  │  - Request permissions                          │    │
│  │  - Access MCP servers for tools                 │    │
│  └────────────────────────────────────────────────┘    │
│                                                          │
│               AI Coding Agent Process                    │
│          (Gemini CLI, Claude Code, etc.)                │
└─────────────────────────────────────────────────────────┘
```

### Is ACP Client-Server or Library?

**Answer: Both, but primarily a subprocess-based library pattern.**

**How It Works:**
1. **Editor (Client)** embeds the `agent-client-protocol` crate
2. **Agent (Server)** also embeds the same crate
3. When user requests agent assistance, **editor spawns agent as subprocess**
4. Communication flows through **JSON-RPC over stdin/stdout pipes**
5. Both sides use the protocol library to handle message serialization/deserialization

**Key Distinction:** Unlike traditional client-server protocols (HTTP, gRPC), ACP agents are:
- Not running as persistent services
- Launched on-demand by the editor
- Communicate via process pipes, not network sockets
- Similar architecture to Language Server Protocol (LSP)

---

## 3. Core Concepts

### Sessions

**What:** Independent conversation contexts with unique identifiers.

**Lifecycle:**
1. **Initialize:** Client and agent negotiate capabilities
2. **Authenticate:** (Optional) Validate credentials
3. **Create Session:** `session/new` establishes a conversation
4. **Prompt Cycle:** Multiple `session/prompt` → `session/update` exchanges
5. **Completion:** Session ends or is canceled

**Session Management:**
```rust
// Creating a new session
pub struct NewSessionRequest {
    pub mcp_servers: Option<Vec<McpServerConfig>>,
}

pub struct NewSessionResponse {
    pub session_id: SessionId,
}

// Loading an existing session (optional capability)
pub struct LoadSessionRequest {
    pub session_id: SessionId,
}
```

### Tools (via MCP Integration)

**Key Concept:** ACP doesn't define its own tool system. Instead, it integrates with MCP.

**How It Works:**
1. Editor exposes MCP servers to the agent during session initialization
2. Agent discovers available tools through MCP protocol
3. Agent invokes tools via MCP's JSON-RPC interface
4. Editor mediates all tool calls (can require user permission)

**Example Tools Available via MCP:**
- Test suite execution
- Documentation queries
- Database access
- File system operations
- Editor commands (find references, rename symbol, etc.)

### Resources (MCP Concept)

**What:** Application-provided contextual data that agents can access.

**ACP's Role:** ACP facilitates passing resource references between editor and agent, but the actual resource retrieval happens via MCP.

**Example Resources:**
- Current file contents
- Selected code snippets
- Project structure
- Git history
- Open buffers

### Prompts

**In ACP Context:** Prompts are user messages sent to the agent via `session/prompt`.

```rust
pub struct PromptRequest {
    pub session_id: SessionId,
    pub prompt: String,
    pub context: Option<Vec<ContentBlock>>,
}
```

**ContentBlock Types:**
- Text content (markdown formatting)
- Image content (base64 encoded)
- Audio content
- Resource links (references to MCP resources)
- Embedded resources (inline content)

### Context Assembly

**How Context is Provided:**
1. **Editor gathers context:**
   - Current file
   - Selected code
   - Open buffers
   - Workspace structure
   - Git status

2. **Context sent with prompt:**
   ```rust
   pub enum ContentBlock {
       TextContent { text: String },
       ResourceLink { uri: String },
       EmbeddedResource { content: Vec<u8>, mime_type: String },
   }
   ```

3. **Agent accesses additional context via MCP:**
   - Query documentation
   - Read files
   - Execute searches

---

## 4. Implementation Details

### Key Rust Types and Traits

#### Core Traits

```rust
/// Trait that agents must implement
pub trait Agent {
    // Required methods
    fn initialize(&self, request: InitializeRequest)
        -> Pin<Box<dyn Future<Output = Result<InitializeResponse>>>>;

    fn authenticate(&self, request: AuthenticateRequest)
        -> Pin<Box<dyn Future<Output = Result<AuthenticateResponse>>>>;

    fn new_session(&self, request: NewSessionRequest)
        -> Pin<Box<dyn Future<Output = Result<NewSessionResponse>>>>;

    fn prompt(&self, request: PromptRequest)
        -> Pin<Box<dyn Future<Output = Result<PromptResponse>>>>;

    fn cancel(&self, session_id: SessionId)
        -> Pin<Box<dyn Future<Output = Result<()>>>>;

    // Optional methods
    fn load_session(&self, request: LoadSessionRequest)
        -> Pin<Box<dyn Future<Output = Result<LoadSessionResponse>>>>;

    fn set_session_mode(&self, request: SetSessionModeRequest)
        -> Pin<Box<dyn Future<Output = Result<SetSessionModeResponse>>>>;
}

/// Trait that clients (editors) must implement
pub trait Client {
    // Required methods
    fn request_permission(&self, request: RequestPermissionRequest)
        -> Pin<Box<dyn Future<Output = Result<RequestPermissionResponse>>>>;

    fn session_notification(&self, notification: SessionNotification)
        -> Pin<Box<dyn Future<Output = Result<()>>>>;

    // Optional methods (capability-gated)
    fn read_text_file(&self, request: ReadTextFileRequest)
        -> Pin<Box<dyn Future<Output = Result<ReadTextFileResponse>>>>;

    fn write_text_file(&self, request: WriteTextFileRequest)
        -> Pin<Box<dyn Future<Output = Result<WriteTextFileResponse>>>>;

    fn create_terminal(&self, request: CreateTerminalRequest)
        -> Pin<Box<dyn Future<Output = Result<CreateTerminalResponse>>>>;

    // ... other terminal methods
}
```

#### Session Management Types

```rust
pub struct SessionId(pub String);

pub struct NewSessionRequest {
    pub mcp_servers: Option<Vec<McpServerConfig>>,
}

pub struct NewSessionResponse {
    pub session_id: SessionId,
}

pub enum SessionNotification {
    MessageChunk {
        session_id: SessionId,
        role: MessageRole,
        content: String,
    },
    ToolCall {
        session_id: SessionId,
        tool_call: ToolCall,
    },
    ToolCallUpdate {
        session_id: SessionId,
        tool_call_id: ToolCallId,
        status: ToolCallStatus,
    },
    Plan {
        session_id: SessionId,
        steps: Vec<PlanStep>,
    },
    // ... other notification types
}
```

#### Tool Execution Types

```rust
pub struct ToolCall {
    pub id: ToolCallId,
    pub kind: ToolKind,
    pub name: String,
    pub arguments: serde_json::Value,
}

pub enum ToolKind {
    Mcp,      // MCP-provided tools
    Editor,   // Editor-specific commands
    Custom,   // Agent-defined tools
}

pub enum ToolCallStatus {
    Pending,
    Running,
    Completed { result: serde_json::Value },
    Failed { error: String },
    Cancelled,
}

pub struct RequestPermissionRequest {
    pub session_id: SessionId,
    pub tool_call_id: ToolCallId,
    pub tool_name: String,
    pub description: String,
    pub risk_level: RiskLevel,
}

pub enum RiskLevel {
    Low,      // Read-only operations
    Medium,   // Write operations
    High,     // Destructive operations
}
```

#### File System Operations

```rust
pub struct ReadTextFileRequest {
    pub path: String,  // Must be absolute
}

pub struct ReadTextFileResponse {
    pub content: String,
}

pub struct WriteTextFileRequest {
    pub path: String,  // Must be absolute
    pub content: String,
    pub create_directories: bool,
}

pub struct WriteTextFileResponse {
    pub success: bool,
}
```

### Communication Protocol

**Message Format:** JSON-RPC 2.0

**Request:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "method": "session/prompt",
  "params": {
    "session_id": "session-123",
    "prompt": "Refactor this function to use async/await",
    "context": [
      {
        "type": "text",
        "text": "```rust\nfn process() { ... }\n```"
      }
    ]
  }
}
```

**Response:**
```json
{
  "jsonrpc": "2.0",
  "id": 1,
  "result": {
    "success": true
  }
}
```

**Notification:**
```json
{
  "jsonrpc": "2.0",
  "method": "session/update",
  "params": {
    "session_id": "session-123",
    "type": "message_chunk",
    "role": "agent",
    "content": "I'll refactor this to use async/await..."
  }
}
```

---

## 5. Integration Pattern

### Where ACP Sits in the Application Stack

**Pattern 1: Direct Integration (CLI with embedded agent)**

```
┌─────────────────────────────────────────┐
│           CLI Application                │
│                                          │
│  ┌───────────────────────────────────┐  │
│  │   UI Layer (Terminal/TUI)         │  │
│  └───────────────┬───────────────────┘  │
│                  │                       │
│  ┌───────────────▼───────────────────┐  │
│  │   ACP Client (ClientSideConn)    │  │
│  └───────────────┬───────────────────┘  │
│                  │ subprocess spawn      │
│  ┌───────────────▼───────────────────┐  │
│  │   ACP Agent (AgentSideConn)      │  │
│  └───────────────┬───────────────────┘  │
│                  │                       │
│  ┌───────────────▼───────────────────┐  │
│  │   LLM Client (API calls)         │  │
│  └───────────────┬───────────────────┘  │
│                  │                       │
│  ┌───────────────▼───────────────────┐  │
│  │   Business Logic Core            │  │
│  └───────────────────────────────────┘  │
└─────────────────────────────────────────┘
```

**Pattern 2: External Agent (CLI as client only)**

```
┌──────────────────────────┐      ┌─────────────────────────┐
│   CLI Application        │      │   External Agent        │
│   (Client Side)          │      │   (Separate Process)    │
│                          │      │                         │
│  ┌────────────────────┐  │      │  ┌───────────────────┐ │
│  │   UI Layer         │  │      │  │ AgentSideConn     │ │
│  └──────┬─────────────┘  │      │  └────┬──────────────┘ │
│         │                │      │       │                │
│  ┌──────▼─────────────┐  │      │  ┌────▼──────────────┐ │
│  │ ClientSideConn     │◄─┼──────┼─►│  Agent Logic      │ │
│  └──────┬─────────────┘  │ stdio│  └────┬──────────────┘ │
│         │                │ JSON │       │                │
│  ┌──────▼─────────────┐  │ RPC  │  ┌────▼──────────────┐ │
│  │ Business Logic     │  │      │  │  LLM Client       │ │
│  │ (File ops, etc)    │  │      │  └───────────────────┘ │
│  └────────────────────┘  │      │                         │
└──────────────────────────┘      └─────────────────────────┘
```

### Does It Require a Separate Server Process?

**No.** ACP uses a subprocess model, not a server model.

**Key Points:**
1. Agent runs as a **subprocess** of the editor/CLI
2. Communication via **stdin/stdout pipes**, not network sockets
3. Agent lifecycle managed by the editor (spawned on demand, terminated when done)
4. Multiple agents can run simultaneously (one subprocess per agent)

### Can It Be Embedded in the Same Binary?

**Yes and No.**

**Scenario 1: Separate Binaries (Typical)**
- Editor binary contains `ClientSideConnection`
- Agent binary contains `AgentSideConnection`
- Editor spawns agent binary as subprocess

**Scenario 2: Single Binary with Multiple Modes (Possible)**
```rust
// Single binary that can run as either client or agent
fn main() {
    let mode = std::env::args().nth(1);

    match mode.as_deref() {
        Some("--agent-mode") => {
            // Run as agent
            let agent = MyAgent::new();
            let connection = AgentSideConnection::new(agent);
            connection.run(stdin(), stdout()).await.unwrap();
        }
        _ => {
            // Run as client
            let editor = MyEditor::new();

            // Spawn self as agent
            let agent_process = Command::new(std::env::current_exe()?)
                .arg("--agent-mode")
                .spawn()?;

            // Connect to agent subprocess
            // ...
        }
    }
}
```

---

## 6. LLM Integration

### Does ACP Include LLM Client Code?

**No.** ACP is LLM-agnostic.

**What ACP Provides:**
- Protocol for editor ↔ agent communication
- Message formats for prompts, responses, tool calls
- Session management
- Permission system

**What ACP Does NOT Provide:**
- LLM API clients (OpenAI, Anthropic, etc.)
- Model selection or configuration
- Prompt engineering utilities
- Token counting or cost tracking

### How Agents Interact with LLMs

**Agent Implementation Responsibilities:**
1. Choose and configure LLM (Claude, GPT-4, Gemini, etc.)
2. Translate ACP prompts into LLM API calls
3. Stream LLM responses back via ACP notifications
4. Handle tool calls from LLM:
   - Parse tool requests from LLM
   - Request permission via ACP
   - Execute tool via MCP
   - Return results to LLM

**Example Flow:**
```
User in Editor → ACP Prompt → Agent → LLM API Call
                                    ↓
                        LLM Response with Tool Call
                                    ↓
                        Agent → ACP Permission Request
                                    ↓
                        User Approves → MCP Tool Call
                                    ↓
                        Tool Result → Back to LLM
                                    ↓
                        Final Response → ACP Update → Editor
```

---

## 7. Key Takeaways

### 5 Critical Points to Understand

1. **ACP is subprocess-based, not server-based**
   - Agents run as child processes of the editor
   - Communication via stdin/stdout JSON-RPC
   - Similar to LSP architecture

2. **ACP complements MCP, doesn't replace it**
   - ACP: How editors talk to agents
   - MCP: How agents access tools/resources
   - Together they create a complete ecosystem

3. **Embedded library pattern**
   - Both editor and agent embed the `agent-client-protocol` crate
   - No separate service or daemon required
   - Can be in same binary or separate binaries

4. **Permission-first security model**
   - All sensitive operations require user approval
   - Editor mediates all agent actions
   - Risk levels guide permission prompts

5. **LLM-agnostic design**
   - ACP doesn't care which LLM you use
   - Agent implements LLM integration
   - Editor only sees ACP protocol

### When to Use ACP

**Good Use Cases:**
- Building a code editor with AI capabilities
- Creating CLI tools that leverage multiple AI agents
- Providing pluggable AI agent support
- Enabling users to choose their preferred AI agent

**Not Ideal For:**
- Direct LLM API integration (use SDK directly)
- Non-coding AI assistants (ACP is coding-focused)
- High-latency environments (subprocess spawn overhead)
- Embedded/mobile applications (resource constraints)

---

## 8. Resources

### Official Documentation
- **Protocol Website:** https://agentclientprotocol.com
- **GitHub Repository:** https://github.com/agentclientprotocol/agent-client-protocol
- **Rust SDK:** https://github.com/agentclientprotocol/rust-sdk
- **Rust Docs:** https://docs.rs/agent-client-protocol

### Reference Implementations
- **Zed Editor:** https://github.com/zed-industries/zed (primary reference)
- **Gemini CLI:** Google's reference agent implementation
- **Goose:** https://github.com/block/goose (Square's ACP agent)

### Related Protocols
- **Model Context Protocol (MCP):** https://modelcontextprotocol.io
- **Language Server Protocol (LSP):** https://microsoft.github.io/language-server-protocol/

---

**End of Report**

*This research provides a comprehensive foundation for understanding and implementing the Agent Client Protocol in Rust-based CLI applications. For the latest updates, always refer to the official documentation at agentclientprotocol.com.*
