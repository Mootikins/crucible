---
title: Footnotes
description: Footnote syntax remains source text; rendering is not implemented
status: planned
tags:
  - reference
  - syntax
---

# Footnotes

Crucible preserves footnote syntax in your markdown, but neither the web UI nor
the TUI renders linked footnotes.

```markdown
A claim[^source].

[^source]: A supporting reference.
```

The unused Rust footnote collection and its duplicate/orphan/unused-definition
diagnostics were retired on 2026-09-14. Inline caret text is no longer extracted
as a separate footnote. Your note files are unchanged.

## See also

- [[Help/Callouts]] — rendered admonition blocks
- [[Help/Block References]] — another unimplemented addressing feature
