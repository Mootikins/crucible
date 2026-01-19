---
description: Making HTTP requests from Lua scripts
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

Lua scripts can make HTTP requests:
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

## Handler Integration

Use HTTP in handlers to fetch external data:

```lua
-- Handler that enriches tool calls with external data
-- @handler event="tool:before" pattern="fetch_prices" priority=10
function on_fetch_prices(ctx, event)
    local response = http.get("https://api.prices.com/latest")
    if not response.ok then
        event.cancelled = true
        event.cancel_reason = response.error
        return event
    end
    event.payload.prices = oq.parse(response.body)
    return event
end
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
    crucible.log("error", "Request failed: " .. response.error)
else
    -- HTTP error (4xx, 5xx)
    crucible.log("error", "HTTP " .. response.status)
end
```

## See Also

- [[Help/Extending/Custom Handlers]] - Handler development
- [[Help/Extending/Creating Plugins]] - Plugin development guide
- [[Help/Lua/Language Basics]] - Lua syntax
