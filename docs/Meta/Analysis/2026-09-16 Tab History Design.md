---
title: Tab History Design — 2026-09-16
description: Back and forward navigation inside a web note tab, with a history tree that the user opens as a git-style graph
tags: [meta, ux, web, design, windowing]
status: draft
updated: 2026-09-16
---

# Tab History Design — 2026-09-16

The comparison target is `crates/crucible-web/web` on `feat/windowing-core` at `fd5a87d74`.

## 1. Goal

A web note tab keeps a navigation history, as a tab in Obsidian does. A link click
opens the target in the current tab. The tab gets a back button and a forward button.
A right-click or a long press on a button shows the history as a tree, in the style
of a git graph.

The same history model will serve a later iframe tab with an editable URL field.
The preview bar of [T3 Code](https://github.com/pingdotgg/t3code)
(`apps/web/src/components/preview/PreviewChromeRow.tsx`) is the reference for that bar.
T3 Code keeps a flat history from Electron `navigationHistory`. It shows no tree.

The TUI has no note tabs, so this feature is for the web only.

## 2. Decisions

| Question | Decision |
|---|---|
| A link points to a note that is open in a different tab | The current tab navigates. Two tabs can show one note. |
| Where the tree lives | In the saved layout, with a cap of 100 nodes. |
| The phone back button | NavStack closes layers first. Then it walks the tab history. |
| The prompt for unsaved changes | An app modal. The web never uses `window.confirm` for this prompt. |
| Where the model lives | In the windowing core, as a typed field. The layout format goes to v11. |
| Opens that are not link clicks (file tree, search, palette) | The same rule as a link: the current tab navigates and gets a history node. `openFileInEditor` has one behavior for all callers. |
| The link gestures in the editor | As in Obsidian. A plain click follows in place. Ctrl/Cmd+click and a middle click open a new tab. Mod-Enter follows in place. A drag that starts on a link still selects text. |
| The `#note=` hash | Only the phone writes it. No code reads it yet. |
| A pinned tab | An open from a pinned tab goes to a new tab. Back and forward still move a pinned tab. |
| How a user pins a tab | Pin tab and Unpin tab in the tab context menu. A pinned tab shows a pin mark. On a phone, a pin toggle on each card of the tab overview. |

The design rejected two other places for the tree:
- `metadata.history` needs no format bump. But the core cannot check the tree there, and each content type must copy the logic.
- A separate store, keyed by tab id, must follow each tab move, split and close. Each of these can cause an error.

## 3. The model

`windowing/model/types.ts` gets `Tab.history?: NavTree`. A node keeps these fields:

```ts
interface NavNode<C extends string = string> {
  id: string;
  parent: string | null;
  title: string;
  contentType: C;
  metadata?: Record<string, unknown>;
  lastVisit: number;
}
interface NavTree<C extends string = string> {
  nodes: Record<string, NavNode<C>>;
  current: string;
}
```

The core does not read `metadata`. Thus a later web entry (`contentType: 'web'`,
`metadata.url`) needs no change in the core.

`windowing/store/tabActions.ts` gets four actions:
- `navigate(tabId, entry)` adds a child below the current node. The tab then takes the title, the `contentType` and the `metadata` of the entry.
- `back(tabId)` goes to the parent node.
- `forward(tabId)` goes to the child with the latest `lastVisit`. A branch that the user leaves stays in the tree.
- `goTo(tabId, nodeId)` goes to any node.

Each move sets `lastVisit` on the node that it reaches.

At 100 nodes, the core removes the leaf with the oldest `lastVisit`. The core never
removes the current node or an ancestor of the current node.

### Tab identity

A tab keeps its identity while its content changes. A new tab gets an opaque id.
An old id such as `tab-file-…` stays valid as an opaque string.

These callers assume one tab for each path, so each one changes:
- `findTabByFilePath` and `host.find` (`lib/file-actions.ts`, `lib/tab-host.ts`) return the active tab of the editor group first.
- `closeTabsUnder` closes a tab only if its current node is under the deleted path. An old node that points under the path gets a "missing" mark.

### Link clicks and other opens

`openNoteInEditor` (`lib/note-actions.ts`) and `openFileInEditor` change:
- A plain click navigates the tab that holds the link. Content that names no tab (the chat, a canvas card) navigates the active editor tab.
- Ctrl/Cmd+click or a middle-click opens a new tab.
- The file tree, a search result, a palette note and every other caller of `openFileInEditor` follow the same rule. They navigate the active editor tab and add a history node.
- If no editor tab exists, the open makes a new tab.
- If the target tab is pinned, the open makes a new tab. A pinned tab that already shows the file only gets the focus. Back, forward and the popover still move a pinned tab.

In the editor, the gestures are the same as in Obsidian:
- A plain click on a wikilink follows it in place. Thus a mouse click cannot put the cursor on a link. The arrow keys can.
- Ctrl/Cmd+click and a middle click open a new tab.
- Mod-Enter follows the link at the cursor in place.
- The link handler takes the press from CodeMirror. If the pointer moves 4 px or more before the release, the press becomes a text selection and follows nothing. Shift and Alt clicks stay with CodeMirror.

This replaces the WS-209 rule, in which a plain click only moved the cursor.

## 4. The saved layout

`LAYOUT_VERSION` in `windowing/model/serializer.ts` goes to 11. `SerializedTab`
gets `history`, and the serializer writes it without icons.

- The migration from v10 gives each tab a tree with one node. That node holds the current content of the tab.
- The reader repairs a bad tree, for example a cycle, a missing parent or a missing `current`. The repair keeps the current content as the only node. This follows the rules of the hardened layout reader.

## 5. The interface

### The bar

The file viewer and the canvas viewer get a thin bar above the content:
- The bar shows a back button, a forward button and the note path.
- The path is plain text now. The iframe tab will change it to an editable URL field later.
- A button with no target is disabled.
- Alt+Left goes back. Alt+Right goes forward. `windowing/shortcuts.ts` holds these keys.

### The tree popover

A right-click or a long press on a button opens the popover. It uses Ark
`Menu.ContextTrigger`, as `TabContextMenu` does in `components/windowing/TabBar.tsx`.

- One column shows the path from the root to the current node.
- Each branch that the user left gets its own lane, with a line to its parent.
- The popover marks the current node.
- A node that points to a deleted file shows in a dim style.
- A click on a node calls `goTo`. The arrow keys move through the nodes.

The graph is a small SVG with one row for each node. A pure function
`layoutNavTree(tree) → rows[]` calculates the lanes, so a unit test can check the
lanes without the DOM.

### Pin a tab

`Tab.isPinned` exists, but no control set it before this design. The design adds one:
- The tab context menu (`TabContextMenu` in `components/windowing/TabBar.tsx`) gets **Pin tab** or **Unpin tab**, above the close rows.
- A core action `setPinned(groupId, tabId, pinned)` writes the flag. The core does not read it.
- A pinned tab shows a pin mark after its title. It keeps its close button.
- The phone has no tab strip and no tab menu. The tab overview is its tab list, so each card gets a pin toggle and the mark. `tabStackStore` already stores and persists the flag.

### The prompt for unsaved changes

If a tab has unsaved changes (`isModified`), `navigate`, `back` and `goTo` stop. An
app modal asks the user to discard the changes. The movement continues only after
the user confirms.

The modal is one app-level component. A promise-based function
`confirmDiscard(filename)` opens it. The web does not use `window.confirm`, for two
reasons:
- An installed PWA can suppress `window.confirm`. A suppressed prompt looks like a broken button (see `components/mobile/TabOverview.tsx`).
- The browser prompt does not use the app theme or the app keys.

The two discard prompts in `contexts/EditorContext.tsx` ask the same question, so
they also use `confirmDiscard`. The other browser prompts in the web client are
outside this design.

## 6. The phone

- Each `navigate` on the phone host adds a NavStack layer (`components/mobile/NavStack.ts`). The `onBack` of that layer calls `back(tabId)`.
- NavStack closes drawers and sheets first, because they are above these layers.
- At the root of the tree, the tab has no layer, so the browser gets the event.
- The browser forward gesture does nothing. On a phone, the forward button and the popover give forward movement.
- `stores/tabStackStore.ts` gets the same actions through `lib/tab-host.ts`.
- `navigate` also writes the `#note=` hash to its own history entry, so the address shows the current note. Only the phone writes the hash. No code reads it yet; a reader is a separate change.

## 7. Errors

- If the file of a node no longer exists, the viewer shows its "missing" state. The tree does not change.
- If the user cancels the discard modal, the tab and the tree do not change.
- The reader repairs a bad saved tree, as section 4 tells.

## 8. Tests

Core unit tests:
- `navigate`, `back`, `forward` (the latest child) and `goTo`.
- The cap. The current node and its ancestors must stay.
- The migration from v10 to v11, and the repair of a bad tree.

`layoutNavTree` tests: a line, a fork and a deep fork.

App tests:
- A plain click navigates in place. A Ctrl+click opens a new tab. The same holds in the editor.
- In the editor, a drag that starts on a link selects text and follows nothing. Shift and Alt clicks stay with CodeMirror.
- The file tree, search and the palette call `openFileInEditor` in its plain form, which navigates in place.
- An open from a pinned tab makes a new tab. Back and forward still move a pinned tab.
- Pin tab and Unpin tab set and clear the flag, and the pin mark follows it. The phone overview toggle does the same.
- The discard modal stops a movement. A cancel keeps the tab unchanged.
- `findTabByFilePath` prefers the active tab.
- `closeTabsUnder` with old nodes.

NavStack test: the phone back button closes a drawer before it moves back in the
tab history.

Playwright tests: a right-click opens the popover. A long press opens the popover.
The buttons are disabled when they have no target. A tab pinned from its menu sends an open to a new tab.

Each new gate must fail once before it passes.

## 9. Documents to change

- [[Web User Stories]] gets the stories for the bar, the popover, the pin control and the phone back button.
- [[Product Decision Log]] records the navigation in place and the end of the rule "one tab for each note".
- WS-209 in [[Web User Stories]] changes: a plain click on a link in the editor follows it.
