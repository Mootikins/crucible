---
title: Footnotes
description: Markdown footnote syntax — what the parser captures and what actually renders
status: partial
tags:
  - reference
  - syntax
---

# Footnotes

Standard markdown footnotes: a reference marker in the text and a definition
elsewhere in the note.

```markdown
Crucible parses footnotes[^1] with named identifiers too[^details].

[^1]: The footnote text.
[^details]: Definitions can sit anywhere in the note.
```

> [!warning] Implementation status
> Footnotes are **parsed, not rendered**. The core parser extracts references
> and definitions into the note's parse result and validates them, but no
> frontend does anything with the syntax: the web UI's markdown pipeline has no
> footnote plugin, so `[^1]` renders as literal bracketed text, and the TUI does
> not treat footnotes specially either. Today footnotes are parse-level metadata
> plus diagnostics.

## Syntax

**References** — `[^id]` anywhere in the text. Identifiers may contain letters,
digits, underscores, hyphens, and spaces.

**Definitions** — `[^id]: content` on its own line (leading whitespace
allowed). Continuation lines indented by at least four spaces or a tab are
folded into the definition, joined with spaces; the first non-indented line
ends it.

```markdown
[^multiline]: First line of the footnote
    continued on an indented line.
```

**Inline footnotes** — a nonstandard caret form: text between two `^`
characters (`some claim^source: the docs^`) is captured as a footnote with a
generated identifier. The text must be at least two characters (`^^` and
`^x^` are ignored), and a `^` directly after `[` is part of a reference, not
an inline footnote.

## What the parser records

References are kept in order of appearance (inline footnotes are appended
after all bracketed references, regardless of position); the first occurrence
of each defined identifier gets a sequential order number (1, 2, …), and
repeated references to the same identifier — and orphaned references — carry
no number of their own. Definitions are keyed by identifier on the parsed
note.

## Diagnostics

Parsing produces per-note diagnostics rather than failing:

- **Duplicate definition** — error; the first definition wins.
- **Orphaned reference** (no matching definition) — warning.
- **Unused definition** (never referenced) — warning.

## See also

- [[Help/Callouts]] — blockquote admonitions
- [[Help/Tags]] — `#tag` organization
- [[Help/Block References]] — another partially-implemented link form
