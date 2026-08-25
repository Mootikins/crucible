---
tags: [llm, agents]
---
# Context Window Management

The context window is finite, so injection competes for tokens. Inject only what
the current turn needs. Cache-friendly ordering puts stable content first.
Summarize older turns rather than dropping them silently.
