---
description: Ideas for storage-agnostic query language for wikilink markdown
status: idea
tags:
  - query-language
  - tooling
  - toon
  - wikilinks
  - self-hosted-llm
related:
  - "[[Help/TOON Format]]"
  - "[[Meta/Ideas/Executable Markdown and Hooks]]"
---

# Storage-Agnostic Query Language

Ideas for querying wikilink-rich markdown without coupling to database/storage layer.

**Key constraint**: Must work well with medium-sized self-hostable models (7B-14B), not just foundation models. Simpler syntax, better training data representation.

## The MarkMiddle Problem

Markdown sits between structured and unstructured:

```
┌─────────────────────────────────────────┐
│  STRUCTURED      │  MIDDLE    │  LOOSE  │
│  (JSON/YAML)     │  (TOON)    │ (prose) │
├─────────────────────────────────────────┤
│  frontmatter     │  blocks    │  text   │
│  typed fields    │  checkboxes│  prose  │
│  arrays          │  wikilinks │  flow   │
│  objects         │  code      │         │
└─────────────────────────────────────────┘
```

Queries need to cross boundaries: "find notes where `type: skill` AND body has `[!]` checkbox AND links to `[[Auth]]`"

## Query Language Options

Evaluated for **medium self-hostable models** (7B-14B). Training data representation matters more than expressive power.

### 1. CSS Selectors for Markdown AST

```css
note[type="skill"] checkbox[status="!"] ~ paragraph
note[type="workflow"] > block > list > item[checkbox]
```

| Aspect | Rating | Notes |
|--------|--------|-------|
| Training data | Excellent | CSS is everywhere, small models know it |
| Syntax simplicity | Good | Familiar, predictable |
| Tree queries | Excellent | What CSS was designed for |
| Graph queries | Poor | Not designed for links |
| Self-host friendly | **Yes** | Simple pattern matching |

**Verdict**: Best for structure queries. Needs extension for links.

### 2. SQL on Virtual Files (DuckDB pattern)

```sql
SELECT * FROM read_md('*.md')
WHERE type = 'skill'
AND '!' = ANY(checkboxes)
AND 'Auth' = ANY(links)
```

| Aspect | Rating | Notes |
|--------|--------|-------|
| Training data | Excellent | SQL is universal |
| Syntax simplicity | Good | Familiar to all |
| Tree queries | Medium | Awkward for nested |
| Graph queries | Medium | CTEs work but verbose |
| Self-host friendly | **Yes** | SQL is well-represented |

**Verdict**: Solid choice. Medium models generate SQL reliably.

### 3. jq Extended with `follow`

```jq
.notes[]
| select(.type == "skill")
| .links[]
| follow
| select(.title == "Auth")
```

| Aspect | Rating | Notes |
|--------|--------|-------|
| Training data | Good | jq known, but niche |
| Syntax simplicity | Medium | Pipeline logic takes practice |
| Tree queries | Good | Native to jq |
| Graph queries | Good | `follow` primitive enables |
| Self-host friendly | **Medium** | Smaller models struggle with jq |

**Verdict**: Powerful but may be too complex for 7B models.

### 4. Datalog (Roam/Logseq pattern)

```datalog
[:find ?title
 :where [?n :type "skill"]
        [?n :links ?target]
        [?target :title "Auth"]]
```

| Aspect | Rating | Notes |
|--------|--------|-------|
| Training data | Poor | Niche, not well represented |
| Syntax simplicity | Medium | Declarative but unusual |
| Tree queries | Good | Pattern matching |
| Graph queries | Excellent | Built for this |
| Self-host friendly | **No** | Small models won't know it |

**Verdict**: Best semantics, worst for self-hosted models.

### 5. Natural Language → Structured

```
User: "skills with blocking requirements that link to Auth"
  ↓ (translation model)
{ type: "skill", checkbox: "!", links: "Auth" }
  ↓ (runs on files)
```

| Aspect | Rating | Notes |
|--------|--------|-------|
| Training data | Excellent | It's just English |
| Syntax simplicity | Excellent | No syntax to learn |
| Accuracy | Medium | Depends on translation |
| Self-host friendly | **Yes** | But adds latency |

**Verdict**: Skip DSL entirely. Small model translates to internal repr.

### 6. XPath for Markdown AST

```xpath
//note[@type='skill']//checkbox[@status='!']/following-sibling::*
```

| Aspect | Rating | Notes |
|--------|--------|-------|
| Training data | Medium | XML era, still known |
| Syntax simplicity | Poor | Verbose, clunky |
| Tree queries | Excellent | Designed for this |
| Graph queries | Poor | Not graph-aware |
| Self-host friendly | **Medium** | Known but disliked |

**Verdict**: Technically correct, practically painful.

## Recommendation for Self-Hosted Models

**Primary**: SQL virtual tables or CSS selectors
- Best training data representation
- Predictable output from 7B-14B models
- Simple enough for reliable generation

**Graph extension**: Add link primitives to SQL
```sql
SELECT * FROM notes
WHERE type = 'skill'
AND links_to('Auth')  -- custom function
```

**Fallback**: Natural language → structured translation
- When syntax generation fails
- Extra latency acceptable for complex queries

## Alternative: Skills as Context Loaders

Skip query DSL entirely. Use embeddings to surface relevant "skills" (context snippets), load them into agent context, let agent reason.

```
Task arrives
  ↓ sparse/dense embeddings
Relevant skills surfaced
  ↓ loaded as context
Agent reasons with full context
  ↓ planning LLM topo-sorts
Checkpoint before execution
```

Query becomes: "what context should I load?" not "how do I express this query?"

## See Also

- [[Help/TOON Format]] - The data format
- [[Meta/Ideas/Executable Markdown and Hooks]] - Tasks as queryable structure
