---
description: How scripts can query LLM agents for decisions
tags:
  - help
  - extending
  - scripting
  - llm
aliases:
  - ask_agent
  - Script LLM Queries
---

# Script Agent Queries

Scripts can query LLM agents to make decisions using structured questions.

## Overview

The `ask.agent()` function lets scripts:
- Ask the LLM structured multiple-choice questions
- Get parsed responses with selected indices
- Handle "other" free-text answers
- Make decisions based on LLM reasoning

This enables script-to-agent communication where another LLM answers questions instead of a human.

## Lua API

```lua
-- Create a question batch
local batch = ask.batch()
    :question(ask.question("Auth", "Which authentication method?")
        :choice("OAuth (Recommended)")
        :choice("JWT")
        :choice("API Key"))
    :question(ask.question("Database", "Which database?")
        :choice("PostgreSQL")
        :choice("SQLite")
        :choice("MySQL"))

-- Ask the LLM agent
local response = ask.agent(batch)

-- Process the response
if not response:is_cancelled() then
    local auth_answer = response:get_answer(0)
    local selected = auth_answer:selected_indices()

    if selected[1] == 0 then
        -- OAuth was selected
        configure_oauth()
    elseif auth_answer:has_other() then
        -- Custom answer provided
        local custom = auth_answer:other_text()
        handle_custom_auth(custom)
    end
end
```

## Question Building

### Creating Questions

```lua
-- Basic question with choices
local q = ask.question("Header", "Question text?")
    :choice("Option A")
    :choice("Option B")
    :choice("Option C")

-- Multi-select question
local q = ask.question("Features", "Which features to enable?")
    :choice("Logging")
    :choice("Metrics")
    :choice("Tracing")
    :multi_select()
```

### Creating Batches

```lua
-- Multiple questions in one batch
local batch = ask.batch()
    :question(q1)
    :question(q2)
    :question(q3)

-- Check batch info
print(batch:question_count())  -- 3
print(batch:id())              -- UUID string
```

## Response Handling

### Answer Structure

Each answer contains:
- `selected_indices()` - Array of selected choice indices (0-based)
- `other_text()` - Custom text if "other" was chosen
- `has_other()` - Boolean indicating if custom text exists

```lua
local answer = response:get_answer(0)

-- Check what was selected
local indices = answer:selected_indices()
for i, idx in ipairs(indices) do
    print("Selected choice " .. idx)
end

-- Check for custom answer
if answer:has_other() then
    print("Custom: " .. answer:other_text())
end
```

### Batch Response

```lua
local response = ask.agent(batch)

-- Check if cancelled
if response:is_cancelled() then
    print("Request was cancelled")
    return
end

-- Iterate answers
for i = 0, response:answer_count() - 1 do
    local answer = response:get_answer(i)
    -- Process each answer
end
```

## Use Cases

### Dynamic Configuration

```lua
-- Let LLM choose configuration based on context
local batch = ask.batch()
    :question(ask.question("Performance", "Optimize for?")
        :choice("Memory efficiency")
        :choice("Speed")
        :choice("Balanced"))

local response = ask.agent(batch)
local choice = response:get_answer(0):selected_indices()[1]

if choice == 0 then
    config.memory_limit = "256MB"
elseif choice == 1 then
    config.workers = 8
end
```

### Decision Trees

```lua
-- Multi-step decision making
local function decide_action(context)
    local batch = ask.batch()
        :question(ask.question("Action",
            "Given context: " .. context .. "\nWhat should we do?")
            :choice("Proceed with caution")
            :choice("Request more info")
            :choice("Abort operation"))

    local response = ask.agent(batch)
    return response:get_answer(0):selected_indices()[1]
end
```

## Rune Equivalent

Rune has the same functionality via `ask_agent`:

```rune
use crucible::ask::{batch, question};

pub async fn main() {
    let b = batch()
        .question(question("Auth", "Method?")
            .choice("OAuth")
            .choice("JWT"));

    let response = ask_agent(b)?;
    let answer = response.get_answer(0)?;
    let selected = answer.selected_indices();
}
```

## See Also

- [[Help/Extending/Custom Handlers]] - Using ask_agent in handlers
- [[Help/Extending/Creating Plugins]] - Plugin development
- [[Help/Lua/Ask Module]] - Full ask module reference
