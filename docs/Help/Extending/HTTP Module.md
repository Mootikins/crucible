---
description: Making HTTP requests from Lua and Rune scripts
tags:
  - help
  - extending
  - scripting
  - http
aliases:
  - HTTP Requests
  - Network Requests
---

# HTTP Module

Make HTTP requests from scripts to interact with external APIs and services.

## Overview

Both Lua and Rune scripts can make HTTP requests:
- GET, POST, PUT, PATCH, DELETE methods
- Custom headers and request bodies
- Configurable timeouts
- JSON response parsing

## Lua API

```lua
-- Simple GET request
local response = http.get("https://api.example.com/data")
if response.ok then
    local data = oq.parse(response.body)
    print(data.name)
end

-- POST with JSON body
local response = http.post("https://api.example.com/users", {
    headers = { ["Content-Type"] = "application/json" },
    body = oq.json({ name = "Alice", age = 30 })
})

-- Custom request with full control
local response = http.request({
    url = "https://api.example.com/resource",
    method = "PUT",
    headers = { Authorization = "Bearer token123" },
    body = "data payload",
    timeout = 60
})
```

### Response Format

All HTTP functions return a table with:

| Field | Type | Description |
|-------|------|-------------|
| `status` | number | HTTP status code |
| `headers` | table | Response headers |
| `body` | string | Response body |
| `ok` | boolean | True if status is 2xx |
| `error` | string | Error message (only on failure) |

## Rune API

Rune uses the built-in `rune-modules` HTTP client:

```rune
use ::http;

pub async fn fetch_data() {
    // Create a client
    let client = http::Client::new();

    // GET request
    let response = client.get("https://api.example.com/data")
        .send()
        .await?;
    let body = response.text().await?;

    // POST with headers and body
    let response = client.post("https://api.example.com/users")
        .header("Content-Type", "application/json")
        .body(r#"{"name": "Alice"}"#)
        .send()
        .await?;
}
```

## Handler Integration

Use HTTP in handlers to fetch external data:

```lua
-- Handler that enriches tool calls with external data
function on_pre_tool_call(event)
    if event.tool == "fetch_prices" then
        local response = http.get("https://api.prices.com/latest")
        if not response.ok then
            return { cancel = true, reason = response.error }
        end
        event.args.prices = oq.parse(response.body)
        return event
    end
    return nil  -- pass through
end

crucible.on("pre_tool_call", on_pre_tool_call)
```

## Error Handling

Always check `response.ok` before using the response:

```lua
local response = http.get(url)
if response.ok then
    -- Success: use response.body
    process(response.body)
elseif response.error then
    -- Request failed (network error, timeout)
    log.error("Request failed: " .. response.error)
else
    -- HTTP error (4xx, 5xx)
    log.error("HTTP " .. response.status)
end
```

## See Also

- [[Help/Extending/Custom Handlers]] - Handler development
- [[Help/Extending/Creating Plugins]] - Plugin development guide
- [[Help/Lua/TOON Query]] - JSON/YAML parsing with `oq`
