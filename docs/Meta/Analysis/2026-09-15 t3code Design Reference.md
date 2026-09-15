---
title: t3code Design Reference — 2026-09-15
description: Index of the T3 Code design study, the ranked changes it proposes for the web UI, and what landed the same day in the sessions rail
tags: [meta, ux, web, design, reference]
status: draft
updated: 2026-09-15
---

# t3code Design Reference — 2026-09-15

[T3 Code](https://github.com/pingdotgg/t3code) is a desktop and web client for coding agents. Its visual design is the reference for the next web UI refinements. Three notes hold the study, each with exact values from the source at commit `3efdcc52` and a comparison with `crates/crucible-web/web` at `7d96a3a81`:

- [[2026-09-15 t3code Design Foundations]] — tokens, type, spacing, borders, motion, icons, primitives, shell. Forty ranked changes.
- [[2026-09-15 t3code Sessions Rail]] — the thread list, row by row. Section 11 ranks the changes; section 13 states the inbox + tree structure.
- [[2026-09-15 t3code Diff Review]] — the changes list, the diff body and the review chrome.

Related: [[2026-09-15 Web UI Review]], [[Web User Stories]], [[Product Decision Log]].

## What landed on 2026-09-15

The sessions rail became an Inbox over a tree. The rule is in [[Product Decision Log]] under the same date.

- The Inbox lists the last five sessions by recency, whatever their status. `lib/session-inbox.ts` holds the one definition, and the phone tab shares it.
- The tree draws only the sessions the Inbox does not. A project whose sessions all sit in the Inbox keeps its header, without a chevron and without a count, so New Session stays reachable.
- A detected project with no session takes no row. The counted "No sessions" fold is gone. "Other projects" remains for the projects a pin scopes out.
- An inbox row names its project in muted text and has no indent. A tree row keeps its indent and takes the project from the header above.
- A group or fold that holds the open session never collapses.

## The P1 list, across the three notes

Foundations, in the order the note gives them:

1. Three row-state surface tokens (hover, active, selected) instead of `bg-primary/15` for a selected row.
2. Status surface washes at 8% light and 16% dark, beside the status text tokens.
3. `tabular-nums` on every timestamp, count, duration and line number.
4. Border-compensated padding on bordered controls.
5. One list-row floor, `--cru-row-sm`, on desktop rows.
6. Menu radii and padding from the role tokens.
7. A 1px inner highlight class for menus, cards and controls.
8. Shared `Button` and `Badge` primitives.
9. Finish the move from `animate-pulse` to the stepped thinking dot.
10. `PanelShell` paints the panel surface; `PanelHeader` gets a fixed height.

Sessions rail:

1. A status vocabulary of six states with a word and a glyph, not a three-value dot.
2. A two-line inbox row with project and status on line one, and the hover slot swapped in place so the row never reflows.
3. A self-ticking elapsed counter on a working row.
4. A confirmation on delete, gated by a setting.
5. A search field in a rail header.

The rail keeps what it already does better than T3 Code: the fill-versus-ring status dot, the kiln shown only on the odd row, per-project New Session, and token discipline.

Diff review:

1. One diff surface with per-hunk syntax highlighting and a virtualized body. Per-line highlighting is wrong past the first line of a multi-line construct today.
2. Diff colour tokens with a `color-mix` ladder, separate from the `ok` and `error` roles, and no colour on the line text.
3. A per-turn changed-files card in the transcript, with inline tool diffs collapsed by default.

The review model stays: the hunk ledger with accept, reject, re-applied, external and superseded has no equivalent in T3 Code, and the unreviewed count is what the write gate holds on.
