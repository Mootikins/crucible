---
title: Plugin Web Delivery
description: How third-party plugin TypeScript reaches the browser, the RPC envelope that follows from the answer, and the trigger that should start the build
type: analysis
status: proposed
updated: 2026-09-10
tags:
  - meta
  - plugins
  - web
  - security
  - analysis
---

# Plugin Web Delivery

Records the delivery decision summarized in [[Meta/Analysis/Plugin API Plan]]: **how does third-party
plugin TypeScript get delivered into the web UI, and what surface follows from
that choice?**

## The recommendation

**Delivery is a sandboxed iframe on an opaque origin with a `MessageChannel`
bridge. The plugin-facing surface is therefore an RPC envelope, not a function
library. Decide that now; build it when the trigger below fires.**

The measurement removes the one technical objection that could have chosen
otherwise. A bridged call costs **under 0.1 ms** more than the same call made
directly, which is below the timer's resolution in a sandboxed frame. Latency
is not a reason to prefer anything else, so the choice falls to containment,
and on containment nothing else competes.

Two consequences follow immediately and cost nothing to adopt today:

- **Do not extract `@crucible/block-api` as a function library.** The
  contract's own argument against it stands, and this settles which way it
  breaks: the package is a `call(method, params)` client over an envelope. A
  library of typed endpoint wrappers would be rework.
- **Do not print a capability label on a web block yet.** Step 4 already says
  this. The bridge is what would make such a label true, and it does not exist.

## What was measured

A spike stands up an origin that replicates `crucible-web`'s own
Content-Security-Policy, frames a plugin on an opaque origin, and proxies
`/api/*` to a live `cru web`. The plugin is real third-party-shaped code: it
cannot see the app, and everything it can do arrives on one port.

Chromium (Playwright), debug daemon, loopback. Median of 60 calls after 10
warm-ups. Code: `scripts/spikes/plugin-bridge/`.

| Call | Direct | Bridged | Cost of the bridge |
|---|---|---|---|
| Bridge alone, no fetch | — | **< 0.1 ms** | — |
| `publications`, 826-byte answer | 44.1 ms | 45.4 ms | **+1.3 ms** (inside run-to-run noise) |
| A 2 000-note graph, 892 KB JSON | 13.5 ms | 18.8 ms | **+5.3 ms** |

The small-payload cost is noise: across four runs the delta moved between
+0.2 ms and +1.3 ms in both directions of the noise band. The large-payload
cost is real and is the structured clone of ~14 000 objects crossing the frame
boundary. It is also avoidable — the host can transfer the response's
`ArrayBuffer` and let the frame parse it, which makes the crossing zero-copy.

**Mount cost, one frame per block.** A note holding many blocks pays per frame:

| Blocks on one page | Total to mount and complete the handshake | Per block |
|---|---|---|
| 1 | 49.1 ms | 49.1 ms |
| 4 | 90.0 ms | 22.5 ms |
| 16 | 153.8 ms | 9.6 ms |
| 32 | 275.5 ms | 8.6 ms |

The first frame pays the cold cost; after that a block costs about 9 ms to
appear. A 32-block note is 0.3 s of mount, which is acceptable and which the
host can defer with an `IntersectionObserver` if it ever is not.

**One caveat on the memory column, stated because the spike cannot close it.**
The host's JS heap did not move measurably across 32 frames, but that figure
covers only the host's heap — each frame has its own, and Chromium's
process-per-frame accounting is outside what this spike can see. Per-frame
*memory* is unmeasured. Per-frame *time* is not.

### The containment actually holds

The plugin frame tried, from inside, what a hostile block would try:

| What it tried | What happened |
|---|---|
| `window.origin` | `"null"` — an opaque origin, as intended |
| `fetch('/api/plugins/publications')` | refused |
| `window.parent.document` | `SecurityError` |
| `document.cookie` | `SecurityError` |
| `localStorage` | `SecurityError` |
| a method not in the host's table | `unknown_method` |

**The opaque origin alone is what refuses the fetch**, and this was worth
separating. A second frame served with no `connect-src` restriction at all was
still refused, by CORS, against `origin 'null'`. So the containment does not
depend on a policy anyone can forget to write; it is a property of the origin.

**But `connect-src 'none'` is not decoration either, and the reason is
precise.** CORS blocks *reading* a response, not *sending* a request. Without
`connect-src`, a hostile frame can still issue simple requests it cannot read,
which is a CSRF-shaped attack. Three separate things already close that gap,
and it is worth knowing that they do:

1. `connect-src 'none'` — the request never leaves the browser.
2. `SameSite=Strict` on the session cookie (`routes/auth.rs`). A document in an
   opaque origin is cross-site, so the cookie is not attached and a blind
   request is unauthenticated.
3. Every gated plugin route takes `Json<…>` and the `PluginCaller` header, so
   every one of them is a *non-simple* request that needs a CORS preflight —
   and a preflight from an opaque origin fails.

Point 3 is a small, pleasing result. The `PluginCaller` header is a decoration
today. The moment the caller is on an opaque origin, the header's mere
existence forces a preflight, so it stops being a label and starts being a
lock — on a route it was never designed to lock.

### What this design does *not* require

**No relaxation of the app's CSP.** `frame-src https: http:` already admits the
frame, because a canvas link node already embeds arbitrary pages the same way.
`script-src 'self'` stays exactly as it is, and plugin JS never executes on the
app origin.

That last sentence is the whole point of choosing this over the obvious
alternative, and it is what keeps the residual in `web/src/pwa-options.ts`
true. That file records that the service worker's root scope is bounded by two
controls, one of which is "the CSP must keep `script-src` to same-origin". A
plugin bundle served as a script would be `'self'`, would satisfy the policy,
and would quietly dissolve that bound — the CSP cannot tell app code from
plugin code. An iframe on an opaque origin never asks it to.

**One CSP detail the implementation must not get wrong.** Inside a sandboxed
document, `'self'` matches no URL, because the origin is opaque. The frame's
own policy must therefore name the serving authority explicitly rather than
say `'self'`, or the plugin's script will not load and the error will not say
why. The spike hit this and it is not obvious from reading the policy.

## The alternatives, and why they lose

### A Web Worker

**Fails on the first requirement.** A worker created from a same-origin script
URL runs *on the app's origin*, with the app's cookies and unrestricted
`fetch`. A `blob:` worker inherits the creating document's origin, so it is the
same. There is no way to obtain a worker in an opaque origin. A worker takes
the DOM away from the plugin and leaves the API wide open, which is the wrong
half.

It composes fine *inside* the iframe — a plugin that wants one can create one —
and it is the right answer to a different question ("keep plugin compute off
the main thread"). It is not an answer to this one.

### A second real origin

Serving plugin bundles from a genuinely different authority — another port, or
a second hostname — also gives isolation, and gives it with a *stable* origin,
so `event.origin` checks work and per-plugin storage is possible.

It loses on three counts. It needs a second listener and a second TLS story in
every deployment, including the proxied one that `HostPolicy` exists for. The
origin is shared by every plugin, so plugin A reaches plugin B's storage — an
opaque origin per frame is strictly finer. And it is configuration that can be
got wrong silently, where an opaque origin is a property of the response.

Worth revisiting only if per-plugin persistent browser storage becomes a
requirement, which nothing has asked for.

### Doing nothing — blocks stay in-tree

This is a real option and it is currently the right one. It costs nothing, it
keeps the `pwa-options.ts` bound intact, and it has shipped two useful blocks.
What it forecloses is a third-party web ecosystem: every block is a pull
request against `crucible-web`, reviewed by us, and a plugin installed from a
git URL can contribute daemon behaviour but never a browser surface.

The honest position is that "do nothing" is right *today* and wrong on a date
nobody has set. So set it — see the trigger.

## The surface: an RPC envelope

The envelope mirrors the split `CLAUDE.md` already names for the events seam —
fan-out with no reply, and correlated one-reply-with-timeout — so a reader who
knows the daemon's event model already knows this one.

**A call, frame to host.** `id` present means a reply is expected.

```json
{ "v": 1, "id": 7, "method": "publications.get", "params": { "key": "kanban:board" } }
```

**An answer, host to frame.** Exactly one per `id`.

```json
{ "v": 1, "id": 7, "ok": true, "value": { "columns": [], "tickets": [] } }
{ "v": 1, "id": 7, "ok": false, "error": { "code": "not_permitted", "message": "…" } }
```

**An event, host to frame.** No `id`, no reply, nothing downstream reads a
return value.

```json
{ "v": 1, "event": "publication_changed", "data": { "key": "kanban:board" } }
{ "v": 1, "event": "theme", "data": { "tokens": { "surface": "#…" } } }
```

**How a call is addressed.** `id` is a frame-local counter. The host keys
pending work per *port*, so two frames cannot collide and no frame needs a
globally unique id. The frame owns the timeout, because the frame is the one
waiting; the host owns a per-port concurrency cap, because the host is the one
that can be flooded.

**How errors and refusals cross.** `error.code` is a closed set, and the
distinction that matters is *who* said no:

| `code` | Who refused | What a UI should do |
|---|---|---|
| `unknown_method` | the host — not in the table | a bug in the plugin; log it |
| `not_permitted` | the host — the plugin's capabilities do not reach this | say so, and name the capability |
| `refused` | the plugin's own command returned a failure | show the plugin's message |
| `unavailable` | the daemon is unreachable | grey out and say why — the contract's cheap half of offline |
| `timeout` | the call outlived its budget | retryable |
| `malformed` | the envelope did not parse | a bug in the client |

`refused` carries the plugin's own answer verbatim, which is where the
contract's item 3 lands: a refusal that names what was skipped and why, in the
shape of `FsMoveOutcome`. Separating `not_permitted` from `refused` is the
load-bearing part — one is a permission answer the user may be able to fix, and
the other is domain logic.

**The method table is a closed set and must be built as one.** `CLAUDE.md` is
explicit about the shape: one enumerated table, an exhaustive match, and a test
that derives its expectation from the running system rather than from source
text — `tools/surface.rs` is the exemplar, and `rpc/dispatch.rs` shows the
generated-from-one-table variant. The bridge's vocabulary must not become two
hand-kept lists on either side of the boundary. Whatever enumerates it should
generate the host's dispatch and the client's declarations from the same place,
the way `signature.rs` already renders one declaration three ways.

## How the identity becomes real

This is the prize, and it is worth being exact about what changes.

Today `routes/plugin_caller.rs` says so itself: a block is same-origin script,
it can set any header, and the identity is asserted rather than proved. The
bridge changes that, and **not** by adding a secret.

**The plugin cannot send an identity, because the envelope has no field for
it.** The host created the frame. It chose the plugin name — from the fence, or
from the panel — loaded *that plugin's* bundle, and closed over that name in
the function that services the port. The header is written there, by app code,
on a request the plugin cannot issue itself. A value with nowhere to go is a
value that cannot be forged.

**The port is a capability, not a name.** `MessageChannel` port2 is transferred
to exactly one frame at creation and nothing else in the browser holds it. So
"which plugin is this" is answered by *which port the message arrived on* — an
object reference, not a string that could be copied. This is the specific thing
the bridge knows that a header does not.

**Origin checks are useless here and the design must not want them.**
`event.origin` is the string `"null"` for every sandboxed frame, so it
distinguishes nothing, and `postMessage`'s `targetOrigin` must be `'*'` because
an opaque origin cannot be named. A design that reached for origin checks would
be building on sand. With a port it never needs to: the host's outbound path is
`window.postMessage(…, '*', [port2])` exactly once, against a
`contentWindow` reference it holds, and every message after that rides the
port.

**What it still does not fix, and why that is now correct.** Step 1 recorded a
second identity problem: `BlockProps.plugin` comes from the fence's first line,
so a note author picks the string a block mounts under. The bridge does not
close that, and should not. What it does is make the choice self-consistent:
today a note author picks a *string* and the app's own code runs under it;
after the bridge, picking `kanban` runs *kanban's code* under *kanban's*
authority. Choosing which plugin to embed is an authoring act, like choosing a
shortcode. Code running under an identity it did not earn is not. The bridge
ends the second and leaves the first, which is the right split.

**And it is the precondition for the web half of step 4.** Because the method
table is a per-plugin closure, the manifest's capabilities can gate entries in
it. That is the first real gate the web side has ever had, and it is what would
make a capability label on a block true rather than decorative.

## What a block loses

### No direct DOM in the host

- **Sizing.** The frame is a box. The block must report its content height over
  the bridge and the host must resize the iframe. A `ResizeObserver` and a
  `resize` event; straightforward, and it flickers if done badly.
- **Popovers and menus cannot escape the frame.** This is the real ergonomic
  cost. A context menu on a kanban card would be clipped at the iframe edge.
  The workaround — the block asks the host to draw a menu — turns a UI detail
  into public API, and should be resisted until something needs it.
- **Keyboard.** App-level shortcuts pressed inside a frame do not reach the
  host. Needs a key-forwarding convention.
- **Text selection, find-in-page and printing do not cross an iframe.** A user
  selecting a paragraph of a note through an embedded block will not get the
  block's text. Nothing today depends on this.
- **Drag and drop across the boundary.** Intra-block dragging is unaffected.
  Dragging *out of* a block into the app would not work. Nothing does this.

### No shared CSS tokens

The block's document is a separate document and the app's stylesheet is not in
it. `KanbanBlock` uses `bg-surface`, `border-hairline`, `text-muted`,
`text-danger` and `border-primary` — Tailwind classes over CSS variables.

The fix is to push the resolved token values over the bridge on init and on
theme change, and have the frame declare them as custom properties. This is
better than it sounds: unlike the Oil spike's sixteen terminal colours, we can
hand over the app's *real* values, so a block can look native.

**The cost is that the token set becomes public API and cannot be renamed.**
That is the single most irreversible decision in this whole design, and it is a
naming decision, not a mechanism one. Fonts have the same shape: the frame must
load them itself, and its `font-src` must name the authority.

### What it costs the two blocks we have

**Kanban ports cleanly.** Its drag and drop is entirely inside the board, so it
survives. It needs the token contract, the resize channel, and one bridge
method for `kanban_move`. A user would not see a difference.

**Graph is the expensive one, and not for the reason one would guess.** The
892 KB clone costs 5 ms and does not matter. What matters is that
`GraphBlock.tsx` imports `openFileInEditor`, `useEditorSafe`, `kilnForPath`,
`noteAbsolutePath` and `listKilns` — it reaches into the app's editor context
to open a note and to learn which note has focus. None of that is plugin data
and none of it crosses a sandbox. It needs three new *host* methods —
`kilns.list`, `app.openNote`, and a `focus` event — and each is a permission
question, not a plumbing one. `app.openNote` lets a block navigate the user's
editor.

That is the finding worth carrying: **the blocks we have do not stop at plugin
data.** The bridge's method table will be as much about what a block may do
*to the app* as about what it may read from the daemon, and that half is
undesigned.

## What it costs to build

Honestly, in pieces. Days are working days for someone who knows the tree.

| Piece | Days | Note |
|---|---|---|
| The bundle route + its CSP | 1 | `routes/plugin.rs`. Serves the plugin's web assets and a synthesized HTML shell, so a plugin never controls its own document's headers. Needs `/api/file/raw`'s path containment and its single-enforcement-point discipline. |
| The gate on that route | 0.5 | A test asserting the CSP carries `sandbox` and never `allow-same-origin`, red-proofed by deletion. |
| The host bridge | 2 | `blocks/bridge.ts`: frame creation, port transfer, the per-plugin method table, dispatch, error mapping, concurrency cap, teardown. `mount.ts`'s disposer must also close the port. |
| Auto-resize | 0.5 | Plus the flicker tuning, which is the part that takes the time. |
| The theme token contract | 0.5 to build, permanent to maintain | The names are the cost, not the code. |
| Host-side app methods | 1–2 | `kilns.list`, `app.openNote`, `focus`. Mostly deciding which are gated. |
| The plugin-side client | 1 | `call(method, params)` over the port, plus typed wrappers. Small *because* the envelope is the contract. |
| Keyboard and focus forwarding | 0.5 | And never quite right. |
| Porting Kanban | 1 | Proves sufficiency, as step 5 already requires. |
| Porting Graph | 2 | The editor-context calls are the work. |
| **Total** | **~10** | Of which about 2 are irreversible design: the token names and the method vocabulary. |

`routes/plugin.rs` and `runtime/plugins/` are owned by other work in flight;
nothing here has touched them.

## The decision, and the trigger

**Decide the mechanism now. Do not build it yet.** Nothing third-party exists,
so ten days of work would buy an authenticated caller in a system with no
third-party callers. What the decision buys immediately is that the work
downstream of it stops being blocked and stops being at risk of going the wrong
way.

**The trigger: a plugin installed through `POST /api/plugins` ships web
assets.** That route already takes an arbitrary git URL. On the day a plugin
arriving that way carries a `web/` directory, "blocks are in-tree" stops being
a policy and becomes a gap.

**Make the trigger a gate rather than a note.** The loader should *refuse* to
serve web assets from an installed plugin, with a message naming this document,
until the bridge exists. Then the trigger fires as a refusal someone reads,
rather than as a memory someone has. That is the difference between a plan and
a note beside the code, and this repository has been bitten by the second
before.

A second trigger, weaker but worth watching: a third in-tree block whose author
would rather not send a pull request against `crucible-web`. Two is a sample of
one team; three is a queue.

## The strongest argument against this recommendation

**The bridge makes the transport honest and leaves the authorization exactly
where it is.**

A block speaking through the bridge under a proved identity still gets whatever
its capabilities get — and step 4 established that `Capability` has ten
variants checked in exactly two places, both `InterceptTools`. `filesystem`,
`kiln` and `config` gate nothing. `Capability::Kiln` cannot say "read the kiln,
write nothing", so a block that needs `getNote` gets `saveNote` with it.

So the ten days would make it impossible to lie about who you are, in a system
where telling the truth already grants nearly everything. That is Obsidian's
position with more machinery — which is precisely the criticism step 4 says we
must not earn.

**An earlier draft answered it with an ordering: enforce first, isolate
second.** That answer is void. The owner ruled Lua-API enforcement out of
scope, and `feat/runtime-path-unification` then deleted nine of the ten
capability variants, so there is no enforcement to put first.

**The objection above therefore stands unanswered, and that is the honest
position.** The ten days buy a proved identity in a system where telling the
truth grants nearly everything. Nothing in this plan closes that gap, because
the gap is a consequence of a ruling rather than a task nobody has done yet.

**Which simplifies the trigger rather than sharpening it.** The bridge is built
when a third-party web block wants in. The second condition — that the
capabilities it would be gated by actually gate something — is struck, because
it can never become true.

## Links

- [[Meta/Analysis/Plugin API Plan]] — settled API decisions and remaining constraints
- [[Meta/Analysis/The Plugin Contract]] — the design being sequenced
- [[Meta/Analysis/Oil in Documents]] — the spike that produced both
- `scripts/spikes/plugin-bridge/` — the measurement, reproducible
- `crates/crucible-web/src/routes/kiln.rs` — the sandbox pattern this copies
- `crates/crucible-web/src/routes/plugin_caller.rs` — the seam this attaches to
- `crates/crucible-web/web/src/pwa-options.ts` — the residual this preserves
