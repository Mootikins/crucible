---
tags: [meta, tui, style]
created: 2026-01-29
---

# TUI Style Guide

Styling preferences for Crucible's terminal UI, rendered by the [[Help/Concepts/Oil]] engine.

## Borders

Use **half-block** Unicode characters for modal and panel borders:

- Top: `▄` (U+2584 Lower Half Block)
- Bottom: `▀` (U+2580 Upper Half Block)

Half-blocks reduce vertical padding compared to full-block characters. The design intent is compact, dense information display.

## Vertical Spacing

Prefer **removing** padding over adding it. When vertical symmetry is needed, eliminate the extra spacer rather than doubling up.

Modal structure (top to bottom):

1. `▄` top border
2. Content (command, prompt, etc.)
3. Single spacer line
4. Options / interactive elements
5. `▀` bottom border (no spacer above)
6. Footer bar (outside the panel)

## Colors

All colors come from `ThemeTokens` — never hardcode RGB values.

| Token | Usage |
|-------|-------|
| `panel_bg` | Modal/panel background fill |
| `overlay_bright` | Primary text inside panels |
| `overlay_text` | Dimmer text (key hints like `[y]`) |
| `text_accent` | Selected/highlighted items |
| `text_primary` | User input text |

## Selection States

- **Selected**: Bold + `text_accent` color, prefixed with `> `
- **Unselected**: `overlay_text` for key bracket, `overlay_bright` for label

## Footer Bar

Sits **below** the bottom border, not inside the panel. Structure:

- Mode badge (inverted, e.g., ` PERMISSION `)
- Type label (bold, e.g., ` BASH `)
- Key/hint pairs: key in accent color, hint in muted color
