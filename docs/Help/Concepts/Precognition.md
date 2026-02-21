---
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

Precognition is Crucible's way of giving your agent a memory. Before every message you send reaches the LLM, Crucible searches your knowledge base for relevant notes and quietly injects them into the conversation. Your agent sees what you've written before, without you lifting a finger.

Think of it as your notes whispering context to the AI right when it needs it.

## Why It Matters

Without Precognition, your agent starts every conversation from scratch. It doesn't know what you wrote last week, what decisions you've made, or what your project looks like. You'd have to copy-paste context manually or hope the AI guesses right.

With Precognition turned on, your [[The Knowledge Graph|knowledge graph]] becomes the agent's long-term memory. Notes you wrote months ago can surface in today's conversation if they're relevant. The more you write, the smarter your agent gets.

## How It Works

The process is invisible. Here's what happens each time you send a message:

1. **You type a message** and hit enter
2. **Crucible searches** your vault using [[Semantic Search|semantic search]], finding notes whose meaning matches your message
3. **Top results get injected** into the prompt as additional context, before the LLM ever sees it
4. **The agent responds** with awareness of your existing notes, links, and ideas

All of this happens in the background. You see a brief notification showing how many notes were found, then the response arrives as usual.

If Precognition finds nothing relevant, it stays quiet and your message goes through unchanged.

## What Gets Searched

Precognition searches at the **block level**, not the document level. Each paragraph, heading section, and list in your notes is indexed separately. This means the agent gets the specific paragraph that's relevant, not an entire 500-line document dumped into context.

The search is semantic. If you ask about "staying productive while remote," Precognition can find notes about "work from home tips" or "focus strategies" even if those exact words don't appear in your message.

## Configuration

Precognition is **on by default**. You can control it from within a chat session using `:set` commands.

### Toggle On/Off

```
:set precognition        # turn on
:set noprecognition      # turn off
:set precognition!       # toggle
```

### Number of Results

Control how many note blocks get injected per message (1 to 20, default is 5):

```
:set precognition.results=3    # inject up to 3 blocks
:set precognition.results=10   # inject up to 10 blocks
```

More results means more context for the agent, but also uses more of the context window. Start with the default and adjust based on how your conversations feel.

### Checking Current Settings

```
:settings
```

This shows all current values, including `precognition` and `precognition.results`.

## When It Activates

Precognition runs on every user message, with two exceptions:

- **Search commands**: Messages starting with `/search` skip enrichment (you're already searching manually)
- **No knowledge base**: If you're running in lightweight mode without a processed vault, there's nothing to search

It doesn't run on system messages, tool outputs, or agent responses. Only your typed messages trigger it.

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

**Keep notes focused.** A note about one topic is more useful than a note about everything. Block-level indexing helps, but focused notes produce cleaner search results.

**Process regularly.** After adding or editing notes, run `cru process` so new content gets indexed. The daemon's file watcher can handle this automatically if configured.

## Precognition vs Manual Search

You can also inject context manually with `/search query` during a chat. Here's when each approach fits:

| | Precognition | Manual Search |
|---|---|---|
| Trigger | Automatic, every message | You type `/search` |
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
