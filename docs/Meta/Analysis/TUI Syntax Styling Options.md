---
title: TUI Syntax Styling Options
description: TUI Syntax Styling Options
tags:
  - meta
  - analysis
---

# TUI Syntax Styling Options

> Design exploration for making markdown elements more visually distinct

## Current State

| Element | Current Style | Issues |
|---------|--------------|--------|
| Blockquote | Gray + DIM, `> ` prefix | Hard to distinguish from prose |
| Wikilinks | Not styled (renders as text) | No visual distinction |
| Tags | Not styled (renders as text) | No visual distinction |
| Callouts | Not implemented in TUI | Missing feature |
| Inline code | Background highlight | OK |
| Code blocks | Syntax highlighted | Good |
| Links | Blue + underline | Good |

---

## Blockquote Options

### Option A: Left Border (Recommended)
```
   │ This is a quoted passage from someone else.
   │ It continues on multiple lines with a
   │ consistent visual indicator.
```
- Uses box-drawing `│` instead of `>`
- Gray/dim text for the content
- Clean, minimal, familiar from GitHub/Slack

### Option B: Italic + Border
```
   ┃ This is a quoted passage from someone else.
   ┃ It continues on multiple lines with a
   ┃ consistent visual indicator.
```
- Thicker border `┃`
- Content in italic
- More emphasis, harder to read long quotes

### Option C: Background Tint (if terminal supports)
```
   ░ This is a quoted passage that has a
   ░ subtle background color to distinguish it.
```
- Subtle background color (if true-color)
- `░` or similar texture character for fallback
- Works well in dark themes

### Option D: Current + Italic
```
  > This is a quoted passage from someone else.
  > It continues with italic text to show
  > it's not the assistant's own words.
```
- Keep `> ` prefix
- Add italic modifier
- Minimal change, more distinct

---

## Wikilink Options `[[Note Name]]`

### Option A: Cyan + Brackets Preserved
```
See [[Project Architecture]] for details.
     ^^^^^^^^^^^^^^^^^^^^^^^^
     cyan, no underline
```

### Option B: Magenta (distinct from links)
```
See [[Project Architecture]] for details.
     ^^^^^^^^^^^^^^^^^^^^^^^^
     magenta/purple, distinguishes from URLs
```

### Option C: Green + Bold
```
See [[Project Architecture]] for details.
     ^^^^^^^^^^^^^^^^^^^^^^^^
     green + bold, "internal link" feel
```

### Option D: Icon Prefix
```
See 📝 Project Architecture for details.
    ^^^^^^^^^^^^^^^^^^^^
    icon + styled text, brackets removed
```

---

## Tag Options `#tag-name`

### Option A: Orange/Yellow (Label-like)
```
Topics: #rust #ai #embeddings
        ^^^^^ ^^^ ^^^^^^^^^^^
        orange text
```

### Option B: Background Pill
```
Topics: #rust #ai #embeddings
        ▌rust▐ ▌ai▐ ▌embeddings▐
        subtle bg, rounded feel
```

### Option C: Dim + Hash Highlighted
```
Topics: #rust #ai #embeddings
        ^     ^   ^
        bright hash, dim tag text
```

### Option D: Cyan (consistent with list markers)
```
Topics: #rust #ai #embeddings
        ^^^^^ ^^^ ^^^^^^^^^^^
        cyan, matches bullet points
```

---

## Callout Options `> [!note]`

### Option A: Icon + Border (GitHub-style)
```
   ℹ️ │ NOTE
      │ This is important information that the
      │ reader should be aware of.
```

### Option B: Colored Border per Type
```
   ┃ NOTE                          (blue border)
   ┃ This is informational.

   ┃ WARNING                       (yellow border)
   ┃ Be careful with this.

   ┃ DANGER                        (red border)
   ┃ This can cause data loss.
```

### Option C: Full-width Header
```
   ╭─ NOTE ────────────────────────╮
   │ This is important information │
   │ that spans multiple lines.    │
   ╰───────────────────────────────╯
```

### Option D: Simple Prefix
```
   [NOTE] This is important information
          that the reader should know.
```

---

## Implementation Recommendations

### Priority 1: Blockquote (Quick Win)
**Recommendation: Option A (Left Border)**

```rust
// In ratatui_markdown.rs blockquote rendering
let border = Span::styled("  │ ", quote_style);  // Was "  > "
```

Changes needed:
1. Update `ratatui_markdown.rs` blockquote prefix
2. Keep existing gray+dim style for content
3. Optional: Add italic modifier

### Priority 2: Wikilinks
**Recommendation: Option B (Magenta)**

Requires:
1. Add `MarkdownElement::Wikilink` to theme.rs
2. Parse wikilinks in ratatui_markdown.rs (regex or markdown-it plugin)
3. Style: magenta fg, preserve brackets

### Priority 3: Tags
**Recommendation: Option A (Orange/Yellow)**

Requires:
1. Add `MarkdownElement::Tag` to theme.rs
2. Parse tags in ratatui_markdown.rs
3. Style: yellow/orange fg

### Priority 4: Callouts
**Recommendation: Option B (Colored Border per Type)**

Requires:
1. Add `MarkdownElement::Callout*` variants to theme.rs
2. Detect callout syntax in streaming parser or markdown renderer
3. Map callout types to colors:
   - note/info → blue
   - tip/success → green  
   - warning/caution → yellow
   - danger/error → red

---

## Color Palette Reference

Using ANSI 256-color indices for terminal compatibility:

| Use | Color | Index | Preview |
|-----|-------|-------|---------|
| Wikilink | Magenta | 5 | Internal reference |
| Tag | Yellow | 3 | Label/category |
| Callout Note | Blue | 4 | Information |
| Callout Warning | Yellow | 3 | Caution |
| Callout Danger | Red | 1 | Critical |
| Blockquote | Gray | 8 | Quoted text |
| Link | Bright Blue | 12 | External URL |

---

## Next Steps

1. [ ] Implement blockquote border change (1 line)
2. [ ] Add wikilink styling (new element + regex)
3. [ ] Add tag styling (new element + regex)
4. [ ] Add callout support (larger feature)
