## Bug Description

When using the Ollama provider with streaming and tool-calling agents, tool calls in the final streaming chunk are dropped. The streaming loop yields `FinalResponse` and breaks before processing any `tool_calls` present in the final chunk.

## Root Cause

In `rig-core/src/providers/ollama.rs` lines 636-682, the streaming loop checks `response.done == true` and immediately yields `FinalResponse` and breaks, without first checking if `response.message.tool_calls` contains any tool calls:

```rust
// Simplified flow showing the bug
loop {
    let response = stream.next().await?;

    if response.done {
        // BUG: Yields FinalResponse and breaks BEFORE processing tool_calls
        yield MultiTurnStreamItem::FinalResponse(...);
        break;
    }

    // Tool calls are processed here - but we already broke out of the loop!
    if let Some(tool_calls) = response.message.tool_calls {
        for tc in tool_calls {
            yield MultiTurnStreamItem::Part(Part::ToolCall(...));
        }
    }
}
```

## Ollama's Behavior

Ollama sends tool calls **only in the final chunk** where `done: true`. This is different from OpenAI which streams tool call deltas incrementally.

Example final chunk from Ollama with tools:
```json
{
  "done": true,
  "message": {
    "role": "assistant",
    "content": "",
    "tool_calls": [
      {
        "function": {
          "name": "read_file",
          "arguments": "{\"file_path\": \"test.txt\"}"
        }
      }
    ]
  }
}
```

## Reproduction

```rust
use rig::providers::ollama;
use rig::agent::Agent;
use rig::tool::Tool;
use futures::StreamExt;

// Create agent with tools
let client = ollama::Client::new("http://localhost:11434");
let agent = client
    .agent("qwen3:8b")
    .tool(ReadFileTool)  // Any tool
    .build();

// Non-streaming works fine
let response = agent.prompt("Read test.txt").await?;
// Returns ToolCall correctly

// Streaming FAILS - tool calls are dropped
let mut stream = agent.stream_prompt("Read test.txt").await?;
while let Some(item) = stream.next().await {
    match item {
        Ok(MultiTurnStreamItem::Part(Part::ToolCall(tc))) => {
            // This is NEVER reached even when model calls tools
            println!("Got tool call: {}", tc.function.name);
        }
        Ok(MultiTurnStreamItem::FinalResponse(_)) => {
            // FinalResponse is emitted immediately without tool calls
        }
        _ => {}
    }
}
```

## Expected Behavior

Tool calls should be yielded as `Part::ToolCall` items before `FinalResponse`, even when they arrive in the final chunk.

## Suggested Fix

Process tool calls BEFORE checking `response.done`:

```rust
loop {
    let response = stream.next().await?;

    // Process tool calls FIRST (they may be in the final chunk)
    if let Some(tool_calls) = response.message.tool_calls {
        for tc in tool_calls {
            yield MultiTurnStreamItem::Part(Part::ToolCall(...));
        }
    }

    // Then check if we're done
    if response.done {
        yield MultiTurnStreamItem::FinalResponse(...);
        break;
    }
}
```

## Workaround

Use `openai::CompletionsClient` with Ollama's OpenAI-compatible endpoint (`/v1/chat/completions`) instead of the native Ollama provider. The OpenAI provider's streaming correctly handles tool calls because OpenAI streams them incrementally rather than in a single final chunk.

## Environment

- rig-core version: 0.27.0
- Rust version: 1.87.0
- Ollama version: Various (tested with Ollama server and llama.cpp's Ollama-compatible endpoint)
- Models tested: qwen3:8b, devstral-small
