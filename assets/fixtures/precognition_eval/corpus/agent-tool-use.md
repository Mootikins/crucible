---
tags: [agents, llm]
---
# Agent Tool Use

Tools let a model act instead of guess. Each tool needs a precise description,
typed arguments, and a permission gate. Failures should return readable errors
so the model can retry differently.
