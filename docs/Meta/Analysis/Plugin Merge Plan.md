---
title: Plugin Merge Plan
description: The work that lands the plugin branch, corrects three defects in the landed code, and makes the documents describe the merged tree
type: plan
status: proposed
updated: 2026-09-10
tags:
  - meta
  - plugins
  - web
  - plan
---

# Plugin Merge Plan

[[Meta/Analysis/Plugin API Plan]] holds no open step. Two reviews of that plan
and of its code found twenty-two defects. This document sequences the work those
findings create.

**The goal.** Land `spike/oil-document-blocks`. Make every document and every
plugin describe what the merged tree does.

## Which tree this plan targets

**This plan targets `feat/runtime-path-unification`, not `master`.**

An earlier draft named master. That was wrong. Master is `b95383032` and it
still holds the `Capability` enum at
`crates/crucible-lua/src/manifest.rs:80`. Every commit this plan depends on
lives on `feat/runtime-path-unification` only: `a7ccf31f3` deletes the
capability system, `92b441c06` and `c3ab023fd` delete `plugin.yaml`,
`28e2302a4` rewrites the config store, `0d3c639b5` deletes `main`.

The conflict count proves which tree each merge produces. A merge of real master
into the spike branch conflicts in **one** file, `plugin_context.rs`. A merge of
`feat/runtime-path-unification` conflicts in **eleven**.

**The dependency.** `feat/runtime-path-unification` must land on master first.
If it does not, steps A, C1 and C4 do not apply, and two residual items return:
`Capability::Kiln` cannot separate read from write, and the `system` variant
maps to nothing.

Name the branch, not a commit. The branch head moved from `660f606f3` to
`5bad89f5c` during this review. The conflict set did not change.

## The fact that reorders everything

`feat/runtime-path-unification` deletes the `Capability` enum. One boolean
replaces it: `intercepts_tools`. That branch's own comment gives the reason the
earlier plan never gave:

> nine names had no call site outside the parser's own tests, and could not have
> had one: `lifecycle/mod.rs` installs `register_stdlib_compat` unconditionally,
> so every plugin holds `io` and `os.remove` whether or not it declared
> `filesystem`.

That branch reached the owner's ruling on its own, then went one step past it.
The owner said the ten names stay declarative. That branch said nine of them
describe nothing, and deleted them.

## Step A — land the branch

**A merge, not a rebase.** Commit `e92e0b277` adds the capability gate and
`38351e093` removes it, so a rebase pays that conflict twice and writes an
intermediate commit that restores a deleted enum. The branch also holds three
merge commits, which a rebase flattens.
`crates/crucible-web/web/src/lib/api.ts` carries six spike commits over a file
the target rewrote, so a rebase conflicts there up to six times against once for
a merge.

### A1. The conflicts are not the work

Git reports 11 conflicted files. The branch holds 82 references to `Capability`,
`CapabilitySet` and `has_capability` across 22 files. Three of those use an
unrelated capability type and do not matter. **Nineteen files use the plugin
enum. Git flags four of them.**

The other fifteen merge without a conflict and then fail to compile. Examples:
`crucible-lua/src/fs.rs:342,357,459`, `crucible-lua/src/executor.rs:646,706`,
`crucible-lua/src/storage_api.rs:279,418`,
`crucible-lua/src/publications.rs:205`,
`crucible-daemon/src/plugin_tools.rs:306,367`. The merged body of
`plugin_context.rs` keeps `use crate::manifest::{Capability, CapabilitySet};`
against a `manifest.rs` that defines neither.

**Budget step A as a 19-file API unwind, not as a resolution of 460 conflicted
lines.**

### A2. One design decision the merge forces

`crates/crucible-daemon/src/plugin_tools.rs:306` and `:367` call
`enter_plugin_without(…, Capability::InterceptTools)`. That call **is** the
attribution fix for plugin tool dispatch. The branch expresses its own work in
the machinery the target deletes.

`plugin_context.rs:166-179` holds the same shape: `enter_plugin_without` takes a
`cap: Capability` parameter.

**Decide the replacement signature before the merge, and record it here.** The
target carries one boolean, so the parameter becomes a boolean that drops the
intercept bit. Without a recorded decision each engineer invents a different
one.

### A3. Three resolution rules, not one

An earlier draft gave one rule: capability machinery yields, the branch's work
survives. That rule is binary, and two conflicts are not.

**Yield to the target.** Capability machinery, subject to A2.

**Union both sides.** `crucible-web/src/test_support.rs`: the target adds four
mock RPC arms (`config.effective`, `config.controls`, `config.origin`,
`config.save`); the branch adds two (`plugin.commands`, `plugin.publications`).
Both insert into one `match`. `crucible-web/web/src/lib/api.ts`: the target adds
`saveConfig`; the branch adds the `PluginCommand` interface and
`getPluginCommands`. Both insert before `getPluginPublications`. The branch side
also carries an orphaned doc comment that needs reattachment.

A engineer who applies the yield rule here deletes one side silently. `api.ts`
has no compiler behind it.

**Mix by hand.** `crucible-lua/src/plugin_context.rs`: one module doc comment,
one hunk. The target changed "a session VM" to "the host itself", because it
deleted the per-session Lua VM. The branch side holds the old phrase plus its
attribution paragraph plus a capability-grants paragraph. Keep the target's
phrase, keep the branch's attribution paragraph, drop the grants paragraph.

### A4. Delete the three `plugin.yaml` files, and move two fields first

The branch adds three: `runtime/plugins/kanban/`, `runtime/plugins/graph/`, and
the fixture that `crates/crucible-daemon/tests/plugin_tools_commands.rs:24`
writes. All three merge without a conflict and then mean nothing.

**A deletion alone breaks `just ci`.** Both YAML files carry
`author: Crucible Contributors` and `license: MIT`. The Lua spec tables carry
neither (`kanban/init.luau:229-233`, `graph/init.luau:172-176`). The target
holds a gate at
`crates/crucible-daemon/src/daemon_plugins/tests/shipped.rs:130` that requires
name, version, description, author and license in the entry file, and
`shipped_plugin_names()` reads `runtime/plugins/` from disk, so both plugins
join the gate with no edit.

Move `author` and `license` into each spec table. Then delete the files.

### A5. Delete the dead declarations

Remove `capabilities` from `runtime/plugins/kanban/init.luau:233` and
`runtime/plugins/graph/init.luau:176`. The target's `lifecycle/spec.rs:160-170`
lists the keys it reads; `capabilities` is absent, and lines 176-179 drop an
unknown key with no error. A kept line is a promise nothing parses.

## Step B — three defects in the landed code

Each defect needs a test. Break the fix, watch the test fail, restore it.

**B1. `GraphBlock` bypasses step 1's seam.**
`crates/crucible-web/web/src/components/blocks/GraphBlock.tsx:99` calls
`runPluginCommand('graph_neighborhood', args)` with two arguments.
`crates/crucible-web/web/src/lib/api.ts:649` defaults the third to `APP_CALLER`.
The block that step 2 built therefore names itself the app, and reaches every
plugin's commands. `KanbanBlock` passes the caller and has a test at
`__tests__/blocks.test.tsx:198`; `GraphBlock.test.tsx` has no equivalent. Pass
the caller. Add the test.

**B2. One attribution test cannot tell a plugin from a tool.**
`crates/crucible-daemon/tests/plugin_tools_commands.rs` names the fixture
directory `shout` at line 21, the plugin `shout` at line 26, and the only tool
`shout` at line 57. The assertion at line 198 is `Some("shout")`. The bug the
test guards is a wrong name, so a context entered under the tool name also
passes. The red-proof succeeds for another reason: no context exists before the
dispatch, so a deleted fix returns `None`. Rename the fixture plugin, or add a
second tool under a plugin with another name.

Step A4 also edits `write_fixture_plugin`. Do A before B2.

**B2a. Two test-packaging defects sit beside B2.** The schedule and timer
attribution tests carry `#[cfg(feature = "send")]`, so a scoped run skips them:
`just test quick -p crucible-lua` runs 1 test of the 3 and prints an unused
import warning at `crates/crucible-lua/src/schedule.rs:285`. A workspace run
enables `send` through `crucible-daemon`, so `just ci` covers all three. This
costs nothing today. It will mislead the next engineer who scopes a run to that
crate.

The schedule test also waits with a fixed `sleep` of 200 ms
(`crates/crucible-lua/src/schedule.rs:427`). The timer test polls. A fixed wait
can produce a false failure, never a false pass, so this is noise rather than a
wrong verdict.

**B3. Kanban rewrites a `status:` line outside the frontmatter.**
`runtime/plugins/kanban/init.luau:103` reads `status:` inside the frontmatter
block only. Line 159 runs its `gsub` over the whole document. A ticket whose
frontmatter carries no `status:` line, and whose body carries one, parses as the
default column and then takes a rewrite in its body. A fenced code block is one
such place.

A second case sits beside it. Line 103 accepts `^status:` on the first line of
the block. Line 159 needs a leading newline. A ticket whose status line opens
the block parses, matches nothing, and falls to the insert path at lines
163-167, which writes a second `status:` key.

Restrict the `gsub` to the frontmatter block. Accept a status line in the first
position.

## Step C — make the documents describe the merged tree

C1 and C4 hold only if `feat/runtime-path-unification` lands first. Read "Which
tree this plan targets" before you start them.

**C1. Rewrite step 4's landed section.** Ten capability names became one
`intercepts_tools` boolean. Record the deletion and its reason.

**C2. Delete the enforcement ordering.** `Plugin API Plan.md:429` says "Enforce
before you isolate". `Plugin Web Delivery.md:404` repeats it and builds its
closing section on it. Line 308 of the same plan marks that work "Ruled out by
the owner". The two sections contradict each other inside one file, and a reader
who starts at step 5 begins work the owner refused.

Step 5's trigger needs a new second condition, or none. Its present second
condition — that the capabilities gate something — can never become true.

**C3. Stop calling path scoping the answer to containment.**
`Plugin API Plan.md:311` calls a scoped read and write "the real work" of step 4.
`crates/crucible-lua/src/luau_compat.rs:306` opens the raw path with no root
check, and `register_stdlib_compat` installs that for every plugin. A plugin that
wants to write outside its roots calls `io.open`. Kanban does exactly that at
`runtime/plugins/kanban/init.luau:78`.

`cru.fs` does scope (`crucible-lua/src/fs.rs:166-196`), so the contrast is real.
Keep both primitives and describe them as ergonomics. Under the owner's ruling a
plugin is trusted code, and no gate binds it.

**C4. Correct the residual list.** Remove the `system` variant and
`Capability::Kiln`; the target deletes both. Add two facts the list omits: every
plugin holds an unscoped `io.open`, and
`crates/crucible-daemon/src/daemon_plugins/mod.rs:464` adds the process
working directory to every plugin's roots. A daemon started from the home
directory grants every plugin the whole home directory through `cru.fs`.

**C5. Split step 2's blocker in two.** The list records one item: a recursive CTE
that returns `(path, hops)`. That mixes two wants of different size. See D1.

**C6. Qualify the anchored-write correction.** `Plugin API Plan.md:446-460` says
kanban's pattern beats a bare-substring `expect` on precision, and names three
losing cases. Kanban wins against a match inside a longer word. It ties inside a
fenced code block, because of the defect in B3. The correction's conclusion
holds: a write primitive needs line-anchored precision and apply-time
re-validation, and the anchored batch supplies only the second.

**C7. Correct seven citations.** Each fact holds; each number is off by one to
four lines.

| Cited | Correct |
|---|---|
| `Plugin API Plan.md:305` | line 308 |
| `daemon_plugins/mod.rs:427-429` | lines 428-430 |
| `vault/mod.rs:664` reads `visible_paths` | line 662 |
| `vault/mod.rs:670` reads `store.graph_links()` | line 671 |
| `vault/mod.rs:691` discards the hop count | line 694 returns `sorted_unique(visited)` |
| `routes/plugin.rs:33-42` registers nine routes | lines 34-42 |
| conflict lines 162, 109, 67, 28, 26 | 162, 107, 65, 26, 24 |

**The gate takes a single line, never a range.** `dev_kiln_code_references_exist`
splits a citation on its last colon and requires the text after it to be all
digits. So `path.rs:1-9` is not a line-anchored citation at all — the whole
string becomes the path, and the file does not exist. Four citations in this
plan were ranges, and CI refused them. Cite the first line instead.

That gate is also what caught the lost `bind_fs_roots` call, indirectly: it
failed on a moved line number, which sent a reader back into the file. No test
covered the loss.

## Step D — two items the review unblocked

**D1. Return the hop count.** `crates/crucible-lua/src/vault/mod.rs:683`
already computes it. The walk carries `(String, usize)`, tests `hops >= depth`,
and enqueues `hops + 1`. Line 694 then discards the number. A return of the
pairs is a change to the return type plus about five lines, and it needs no
storage work. It removes the per-hop call the plugin makes.

The indexed query is the separate half, and it is real storage work. Lines 662 and 671 read `visible_paths` and `store.graph_links()` in full on every call, so
a smaller result saves the wire and saves the daemon nothing. Step 2's negative
verdict does not depend on either half, because depth 1 already costs 58 per
cent of a whole-graph fetch.

**D2. Record step 5's trigger where a reader stands.** An earlier draft made the
install path refuse a plugin that ships `web/`. Three objections beat that
form. No plugin holds a `web/` directory, and
`crates/crucible-web/src/routes/plugin.rs` serves no bundle, so the assets are
already unreachable. A directory test refuses a third-party plugin that keeps
unrelated sources in `web/`. The target added `cru.rtp`, so a plugin can arrive
through a runtime root and miss the install path.

Write the rule as a comment beside the route table at
`crates/crucible-web/src/routes/plugin.rs:34`, where the engineer who would
add a bundle route reads it.

## Order

**Step A first.** Every other item conflicts more after the target moves again.

**B and C run beside each other after A.** No B or C item makes step A harder.
B2 and A4 both edit `write_fixture_plugin`, so A comes first.

**D runs beside B.** D1 edits `crucible-lua/src/vault/mod.rs` and D2 edits
`crates/crucible-web/src/routes/plugin.rs`. Neither file appears in B.

## The residual list after this plan

- Kanban writes with `io.open`, not `cru.fs.write`.
- `LuaType` has no string-literal type, so a generated form gets no dropdown.
- `scoped_neighbors` reads the whole note list and the whole link table on every
  call. An indexed query is storage work.
- The web bridge waits on a trigger.
- Every plugin holds an unscoped `io.open`.
- Every plugin holds the process working directory as a root.

## Open, and unconfirmed

- **Does the branch call a session-VM API the target removed?** The target
  deleted the per-session Lua VM in `f4b6001b8`, `1eabcfe90` and `e64dd5478`.
  Nobody compiled the merged tree, so nobody knows. Find out during step A.
- ~~Findings 1 to 5 of the first review reached no file.~~ **Closed.** All five
  reach this plan. Finding 1 reaches four places at once, which is why four
  steps carry five findings. Two items they raised were not carried and now are,
  as B2a.

## Links

- [[Meta/Analysis/Plugin API Plan]] — the plan this succeeds
- [[Meta/Analysis/The Plugin Contract]] — the design both sequence
- [[Meta/Analysis/Plugin Web Delivery]] — step 5's evidence
