---
title: Callouts
tags:
  - help
  - editor
---

# Callouts

Callouts turn a blockquote into a colored admonition block, using the same
syntax as Obsidian: start a blockquote with `[!type]` and an optional title.

```markdown
> [!note] Optional title
> Body content, with **markdown** and [[Wikilinks]].
```

Append `-` for a foldable callout that starts collapsed, or `+` for one that
starts open:

```markdown
> [!tip]- Click to expand
> Hidden until unfolded.
```

## All variants

> [!note] Note
> The default. Aliases: none.

> [!abstract] Abstract
> Aliases: `summary`, `tldr`.

> [!info] Info
> Plain informational block.

> [!todo] Todo
> A checklist-flavored block.

> [!tip] Tip
> Aliases: `hint`, `important`.

> [!success] Success
> Aliases: `check`, `done`.

> [!question] Question
> Aliases: `help`, `faq`.

> [!warning] Warning
> Aliases: `caution`, `attention`.

> [!failure] Failure
> Aliases: `fail`, `missing`.

> [!danger] Danger
> Alias: `error`.

> [!bug] Bug
> For known defects.

> [!example] Example
> Worked examples and demos.

> [!quote] Quote
> Alias: `cite`.

## Foldable

> [!tip]- Collapsed by default
> You found the hidden content.

> [!question]+ Open by default
> Fold me away if you like.

## Notes

- Unknown types (e.g. `[!custom]`) fall back to `note` styling, matching
  Obsidian.
- The title defaults to the capitalized type when omitted.
- Callouts render in reading mode, chat messages, and hover previews; in the
  live-preview editor the block is tinted with the variant color while
  staying editable.

