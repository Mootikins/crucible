---
title: Precognition
description: How Crucible automatically surfaces relevant notes during conversation
status: implemented
tags:
  - precognition
  - rag
  - context
  - embeddings
  - knowledge-graph
---

# Precognition

Precognition is Crucible's way of giving your agent a memory. Before your opening message reaches the LLM, Crucible searches your knowledge base for relevant notes and quietly injects them into the conversation. Your agent sees what you've written before, without you lifting a finger.

Think of it as your notes whispering context to the AI right when it needs it.

## Why It Matters

Without Precognition, your agent starts every conversation from scratch. It doesn't know what you wrote last week, what decisions you've made, or what your project looks like. You'd have to copy-paste context manually or hope the AI guesses right.

With Precognition turned on, your [[The Knowledge Graph|knowledge graph]] becomes the agent's long-term memory. Notes you wrote months ago can surface in today's conversation if they're relevant. The more you write, the smarter your agent gets.

## How It Works

The process is invisible. Here's what happens on the first message of a session (see [When It Activates](#when-it-activates)):

1. **You type a message** and hit enter
2. **Crucible searches** your vault using [[Semantic Search|semantic search]], finding notes whose meaning matches your message
3. **Top results get injected** into the prompt as additional context, before the LLM ever sees it
4. **The agent responds** with awareness of your existing notes, links, and ideas

All of this happens in the background. You see a brief notification showing how many notes were found, then the response arrives as usual.

If Precognition finds nothing relevant, it stays quiet and your message goes through unchanged.

## What Gets Searched

Precognition retrieves at the **note level**. During indexing, each block of a note is embedded and those block embeddings are averaged into one document vector per note; retrieval matches your message against those note vectors. What gets injected is the matching notes, so focused, single-topic notes surface more cleanly than sprawling ones.

The search is semantic. If you ask about "staying productive while remote," Precognition can find notes about "work from home tips" or "focus strategies" even if those exact words don't appear in your message.

Every kiln connected to the session is searched, not just the primary one. Results from all connected kilns are merged and ranked together, with two per-kiln guards: a kiln whose data classification exceeds the session provider's trust level is skipped entirely (see [[Trust and Classification]]), and a kiln indexed with a different embedding model than the current provider is skipped rather than compared against incompatible vectors.

## Configuration

Precognition is **on by default**. You can control it from within a chat session using `:set` commands.

### Toggle On/Off

```
:set precognition        # turn on
:set noprecognition      # turn off
:set precognition!       # toggle
```

### Number of Results

Control how many notes get injected per message (1 to 20, default is 5):

```
:set precognition.results=3    # inject up to 3 notes
:set precognition.results=10   # inject up to 10 notes
```

More results means more context for the agent, but also uses more of the context window. Start with the default and adjust based on how your conversations feel.

### Checking Current Settings

```
:settings
```

This shows all current values, including `precognition` and `precognition.results`.

### Customizing with Lua

Plugins can reshape Precognition through two event seams:

- `precognition_select` — runs after retrieval, before injection. A handler sees the retrieved notes and chooses which to keep (filter, reorder, cap).
- `precognition_format` — controls how the selected notes are rendered into the injected context block, replacing the default formatting.

Register handlers with `crucible.on("precognition_select", ...)` / `crucible.on("precognition_format", ...)`. See [[Help/Extending/Event Hooks]] for handler signatures and semantics.

## When It Activates

Precognition runs on the **first user message of a session** — not on every
turn. Re-injecting on each turn bloats the context, costs prompt-cache hits, and
mostly surfaces notes the opening injection already covered. Follow-ups in the
same conversation are usually about the same topic.

Three things stop it running even on a first message:

- **Turned off**: `:set noprecognition` (see [Toggle On/Off](#toggle-onoff))
- **Search commands**: Messages starting with `/search` skip enrichment (you're already searching manually)
- **No knowledge base**: If you're running in lightweight mode without a processed vault, there's nothing to search

It doesn't run on system messages, tool outputs, or agent responses. Only your typed messages trigger it.

Because of the first-message rule, `:set noprecognition` part-way through a
conversation has nothing left to prevent in *that* conversation — injection
already happened on your opening message. The setting sticks to the session, so
it takes effect on the next conversation, on a session you rewind with `:undo`,
and for any other client attached to the same session.

## Requirements

For Precognition to work, you need:

1. **A processed vault**: Run `cru process` on your notes at least once so embeddings exist
2. **An embedding provider**: Crucible needs a way to generate embeddings (Ollama, FastEmbed, or OpenAI)
3. **Notes worth finding**: The more you write and link, the better Precognition gets

If embeddings aren't available, Precognition silently disables itself. Your chat still works, just without the automatic context injection.

## Tips for Better Results

Precognition is only as good as your notes. A few habits make a big difference:

**Write notes you'd want to find later.** Clear titles, descriptive paragraphs, and specific details all help semantic search find the right content.

**Use wikilinks.** Links between notes strengthen the [[The Knowledge Graph|knowledge graph]]. When Precognition finds one note, related linked notes become easier to surface too.

**Tag your notes.** Tags in frontmatter help organize your vault and give Precognition more signal about what a note covers.

**Keep notes focused.** A note about one topic is more useful than a note about everything. Each note gets a single document vector, so a note that covers many topics dilutes its own signal.

**Process regularly.** After adding or editing notes, run `cru process` so new content gets indexed. The daemon's file watcher can handle this automatically if configured.

## Precognition vs Manual Search

You can also inject context manually with `/search query` during a chat. Here's when each approach fits:

| | Precognition | Manual Search |
|---|---|---|
| Trigger | Automatic, first message of a session | You type `/search` |
| Effort | Zero | You choose the query |
| Precision | Good for general relevance | Better when you know what you want |
| Control | Background, hands-off | You see results and pick what to include |

They work well together. Let Precognition handle the background context while you use `/search` for specific lookups.

## Troubleshooting

**Agent doesn't seem to know about my notes**
- Check that Precognition is on: `:settings` should show `precognition: true`
- Make sure you've run `cru process` to generate embeddings
- Verify an embedding provider is configured

**Too much irrelevant context**
- Lower the result count: `:set precognition.results=2`
- Your notes might need clearer, more focused content

**Responses are slow**
- Embedding lookup adds a small delay before each response
- If using a remote embedding provider, network latency adds up
- Try a local provider like FastEmbed for faster lookups

## See Also

- [[Semantic Search]] - How meaning-based search works
- [[The Knowledge Graph]] - How wikilinks create structure
- [[Plaintext First]] - Why markdown files are the source of truth
