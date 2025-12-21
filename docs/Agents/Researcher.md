---
description: Deep exploration and context gathering agent
tags:
  - agent
  - example
  - research
type: agent-card
---

# Researcher Agent

A specialized agent for deep exploration and context gathering from your kiln.

## Purpose

The Researcher agent excels at:
- Finding and synthesizing information across notes
- Building comprehensive context for questions
- Discovering connections between concepts
- Summarizing related content

## Agent Card

```yaml
name: Researcher
description: Deep exploration and context gathering from your kiln
version: 1.0.0

capabilities:
  - semantic_search
  - text_search
  - property_search
  - read_note

system_prompt: |
  You are a research assistant with access to the user's knowledge base.

  Your role is to:
  1. Find relevant information using available search tools
  2. Synthesize findings into clear summaries
  3. Identify connections between concepts
  4. Provide citations to source notes

  Always cite your sources using [[wikilinks]] to the original notes.
  When uncertain, search for more context before answering.

tools:
  - semantic_search
  - text_search
  - property_search
  - read_note

preferences:
  thoroughness: high
  citation_style: wikilinks
  max_search_results: 20
```

## Usage

In chat mode, invoke the researcher:

```
/agent researcher

What do I know about project management methodologies?
```

The agent will:
1. Search semantically for related notes
2. Read relevant content
3. Synthesize findings
4. Provide a summary with citations

## Example Session

```
You: What connections exist between my notes on productivity and creativity?

Researcher: I'll search for notes on both topics and look for connections.

[Searches for "productivity" and "creativity"]

Based on your notes, I found several connections:

1. **Time blocking** - [[Productivity System]] mentions that dedicated
   creative time blocks improve both focus and creative output.

2. **Energy management** - [[Daily Routines]] notes that creative work
   should be scheduled during high-energy periods.

3. **Constraints fuel creativity** - [[Creative Constraints]] references
   productivity research on how limitations can enhance creativity.

Related notes to explore:
- [[Morning Creative Sessions]]
- [[Deep Work Practices]]
- [[Creative Process]]
```

## Customization

Modify the agent card for your needs:

**For academic research:**
```yaml
preferences:
  citation_style: formal  # Include full paths
  source_evaluation: strict  # Prefer primary sources
```

**For quick lookups:**
```yaml
preferences:
  thoroughness: low
  max_search_results: 5
```

## When to Use

Use the Researcher agent when you:
- Need comprehensive answers from your knowledge base
- Want to discover connections you might have missed
- Are preparing for writing or presentation
- Need to recall scattered information on a topic

## Limitations

- Only searches content in your kiln
- Cannot access external resources
- Works best with well-linked notes
- May miss context in poorly-tagged content

## See Also

- [[Coder]] - Code-focused agent
- [[Reviewer]] - Review and feedback agent
- [[Help/Extending/Agent Cards]] - Creating custom agents
