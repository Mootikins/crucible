---
title: Note Sync
description: How a note write reaches the daemon, what happens when two writers change one note, and how you settle a conflict
status: implemented
tags:
  - notes
  - web
  - offline
  - sync
---

# Note Sync

Crucible is plaintext first. A note is bytes on disk, and the daemon owns the
write. The agent writes notes. You write notes. Another device writes notes. So
two writers reach one note often, and this page says what happens when they do.

Every note write goes through one door, whether you typed it in the browser or
an agent wrote it through a tool call. The door is the same on a desktop and on
a phone.

## What a save carries

A save carries three things: the text you want, the **base** (the hash of the
note as you read it), and the **base text** (the note as you read it).

The base is what makes a save safe. If the note on disk no longer hashes to your
base, another writer changed it since you read it, and a blind write would erase
their work. The daemon refuses that write.

The base text is what makes the refusal recoverable. With all three texts the
daemon merges instead of refusing.

## When the note moved on

The merge is a three-way merge, line by line, over your base text, your text and
the text on disk. The write path holds a lock on that one note across the read,
the compare, the merge and the write, so two writers are ordered rather than
raced.

There are three answers:

- **A clean merge.** You each changed different lines. Both changes land, and
  your buffer takes the merged text. You see no question.
- **A merge with regions.** You both changed the same lines, differently.
  Nothing is written. The answer carries the merged text and one **region** per
  disputed line group, each holding the base, your text and theirs.
- **A refusal.** The write carried no base text, so there was nothing to merge
  from. Nothing is written. Reload the note and make the change again.

## A conflict waits where you can find it

A merge that left regions becomes a **conflict**. The write is not lost and it is
not written beside the note under another name. It waits in the outbox, holding
the merged text and its regions, until you settle it.

Three surfaces open the same Conflicts panel:

- the app bar badge on a phone, which counts conflicts apart from unsent edits,
  shows even when the queue is empty, and opens the conflict when you tap it;
- the **Conflicts** row in the phone's More sheet, which appears only while one
  waits;
- the Changes panel, which lists every conflict above the agent's hunks, with no
  session selected.

The panel lists what waits. Open one and the merged text is drawn in the editor,
with a block at every region. Each block shows both texts, marks the words that
differ, and offers three choices:

- **Keep mine** — your text.
- **Keep theirs** — the text on disk.
- **Keep both** — yours, then theirs.

A counter says how many regions you have settled. Save stays disabled until you
settle every one. Leaving with a region open asks first.

Save writes the settled text against the hash the daemon answered with. If the
note moved again while you were choosing, the save is refused and the conflict
stays open.

## Editing offline

When the daemon does not answer, the write is queued in the **outbox** instead.
The app bar counts what is unsent, and sends it when the connection returns.

- The outbox holds one entry per note. A later write to that note folds into the
  entry, and the fold keeps the base and the base text of the first write.
- A queued write keeps its base text too, so a drain that meets a changed note
  merges rather than refusing.
- An entry the merge could not settle becomes a conflict and stops being sent.
  Sending again cannot settle it; only you can.
- A write to a note that already holds a conflict replaces the conflict with your
  new write and its own base.

A write the daemon *answered* and refused is never queued. A refusal is an
answer, and the outbox is for writes that got none.

## A note that changed while you have it open

The editor listens to the kiln watcher while a note is open.

- If your buffer is **clean**, it re-reads the note quietly and shows the new
  text.
- If your buffer is **dirty**, nothing is taken from you. A banner says
  `This note changed on disk — your unsaved edits are still here`, with two
  buttons.

**Reload** takes the text on disk. It asks first, because the text in your buffer
exists nowhere else.

**Merge** saves. The save carries your base text, so the ordinary case merges and
lands. A merge that leaves regions opens the conflict view.

## What this never does

- It never overwrites another writer silently. A write whose base moved is
  merged or refused, never applied blind.
- It never writes a second note beside yours. An earlier version of Crucible kept
  a stale write as a dated copy next to the note. Nothing listed those copies, so
  people met them by accident or never. A conflict is counted, listed and opened
  instead.
- It does not use a CRDT. The truth is markdown on disk, which the agent, the
  TUI, the CLI and any other editor may rewrite. A CRDT needs every writer to
  speak it, and most writers here never will.

## Related

- [[Help/Concepts/Kilns]] — where notes live
- [[Help/Concepts/Review Ledger]] — how an agent's own writes are disposed
- [[Help/Concepts/Plaintext First]] — why the bytes on disk are the truth
- [[Help/Config/web]] — configuring the browser UI this happens in
