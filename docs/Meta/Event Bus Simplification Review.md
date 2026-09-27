---
title: Event Bus Simplification Review
description: Concrete opportunities to consolidate event ownership, routing, and recovery
tags: [meta, architecture, review]
status: in-progress
---

# Event Bus Simplification Review

Reviewed the September 26 working tree, including the three fixes in
[[Meta/September 26 Review Follow-up]]. These are code-inspection findings and
proposed refactors, not newly reproduced runtime failures. Ownership follows
[[Meta/Architecture/Index]] and [[Meta/CONTEXT]].

The useful direction is one publication contract and explicit subscriber
lifetimes. JSON-RPC, SSE, ACP and MCP still need their own transport policies.
Commands that need answers keep correlation, timeout and cancellation;
synchronous interception stages stay separate from broadcast observers.

## 1. Finish the system stream migration, then share gap recovery

**Implemented:** publication blocks and proposal readers now share the system
root and decoder. The system route owns publication invalidation; the server
alias remains available. Connection sharing and publication routing regressions
were observed failing before the change. All system projections now share subscription, handshake, keepalive and gap
forwarding. Browser caches reconcile open/reopen and gaps; Chromium verifies
native retry without a subsequent change event. Recovery cancels an initial
fetch before refetching; ordinary invalidation can reuse a snapshot started
before the gap. New readers wait through disconnection before seeing open.

**Small first step.** `crates/crucible-web/src/routes/events.rs` already serves
both publication and proposal changes on `/api/events/system`. The plugin
endpoint is explicitly a compatibility alias. Nevertheless,
`web/src/lib/query/sse.ts` keeps separate plugin and system roots, and
`web/src/lib/query/routes/system.ts` deliberately ignores publication changes
because `routes/plugins.ts` handles them through the other stream.

Move publication consumers to the system root and route publication changes
there. Keep the server alias for compatibility. This removes one browser
connection and its duplicate subscription/decoder/route machinery when the
Inbox and a publication block are open together.

**Follow-on.** Filesystem and surface routes also subscribe to `system`.
`routes/events.rs:116`, `routes/fs.rs:257` and `routes/surface.rs:228` discard
`BroadcastStream` errors with `result.ok()`. Chat instead converts lag into
`stream_gap` in `routes/chat.rs:176`. A dropped change can leave a cache stale
until some other action refreshes it.

Share subscription setup, handshake, keepalive and loss reporting; let each
projection retain its payload shape during migration. Eventually the browser
can use one system feed with typed filtering for files, surfaces, proposals
and publications. A gap or reconnect should invalidate the affected query
families; system events do not all have replayable history.

**Gates:** two consumers open one source; overflow a small ring and observe
reconciliation; disconnect during a change and recover without a subsequent
change; preserve the legacy endpoint's publication-only contract. Native
EventSource reconnect behavior must be tested: comments in `lib/api.ts` claim
the plugin/system streams do not reconnect, but their implementations install
no error handler that disables the browser's native retry.

## 2. Give the web broker ownership of subscription lifetimes

**Implemented:** stream leases own local reception and upstream interest.
The first reader subscribes and the last releases; reconnect restores every
active receiver and announces the unknown lost span before forwarding new
events. Initial failure and cancellation release interest. Failed restoration
does not advance the connection generation. Socket/SSE regressions and mutation
checks cover these paths.

`routes/chat.rs:142` creates a local receiver and separately subscribes the
daemon. `services/daemon.rs:153` reconnects the daemon connection and restores
only `sticky_subscriptions`; its comments expect chat subscriptions to be
restored by browser EventSource reconnects. The existing broker channels and
SSE responses survive the router replacement, however. A daemon reconnection
does not itself force those browser connections to reopen. This is a concrete
recovery path to reproduce before changing it.

`EventBroker::subscribe` at `services/daemon.rs:800` also retains senders until
explicit session removal. Dropping the last SSE receiver does not unregister
its upstream subscription. This means two structures describe subscription
lifetime, neither completely.

Return a subscription lease that owns local reception and upstream interest.
Create the local receiver first, subscribe upstream on the first interest,
restore all active interests on daemon reconnect, and release them on the last
lease. Keep the intentionally permanent system interest explicit. Coordinate
release and reconnect through one owner so an old unsubscribe cannot remove a
new subscription.

**Gates:** retain two browser streams while restarting/reconnecting the daemon;
both receive the next event without browser reload. Drop one stream and keep
the other alive. Drop the last and reclaim state. Exercise subscribe/drop
during reconnect and failed initial subscription.

## 3. Make the daemon bus a concrete owned value

`crates/crucible-daemon/src/event_emitter.rs` exposes a raw broadcast sender,
but publishing also depends on process-global sequence counters (line 7), a
global weak-channel-to-journal registry (line 95), and a registry lookup for
each publish (line 98). Session cleanup explicitly removes sequence entries.
`server/mod.rs:214` constructs the channel and then attaches its journal.

A cloneable concrete `EventBus` handle can own the sender, sequence allocation
and journal together. Publishing, live subscription, journal consumption and
session retirement become methods on that owner. It removes the global channel
lookup and prevents an accidentally unjournaled raw send in production. It
also makes independent daemon instances independent in tests.

Preserve the existing atomic ordering of sequence assignment, journal enqueue
and broadcast. Preserve the lossless journal versus lossy live ring distinction;
making every subscriber lossless would change memory and backpressure behavior.
Keep recorded replay explicit so original sequence numbers are retained.

**Gates:** concurrent emitters produce the same sequence/journal/broadcast
order; no live receivers still journals; two buses using the same session name
remain independent; retirement and late emitters obey the lifecycle contract.

## 4. One owner for entity invalidation rules

**Implemented:** `proposal-cache.ts` owns reconciliation for mutations and
events. Same-microtask requests share a batch and its completion promise; a
change arriving after fetching starts creates another batch so an old response
cannot hide it. Split replies reconcile both proposal ids. Tests cover both
event/reply orders and the no-event fallback.

`web/src/lib/query/proposals.ts:74` and `routes/system.ts:22` independently name
the same proposal, proposal-list and diffset query keys. One mutation can
invalidate through its response and again through the emitted daemon event.
The session route already has an isolated debounce for review invalidation.

Extract an entity-level invalidation function and use it from both paths.
Coalesce simultaneous invalidations by key while preserving the mutation's
promise: callers currently wait until their reconciliation completes. Start
with proposals, where the duplicate rules are exact and the split reply can
name a second proposal.

Keep mutation reconciliation as a fallback until reconnect/gap recovery is
reliable. Replacing it with "the event will arrive" would simplify the code by
weakening correctness. A refused mutation and an ambiguous transport outcome
also need different treatment from a confirmed successful event.

**Gates:** event-before-response and response-before-event converge; split
decisions refresh both ids; another client's decision updates the view; an
absent event cannot leave the initiating client's cache permanently stale.

## 5. Replace source-scanning event completeness tests with derived metadata

**Implemented:** a shared declaration generates serde tags and routing metadata
from each wire name. Group routing is exhaustive, and completeness tests inspect
compiled metadata instead of Rust source. Recorded fixtures and malformed/unknown
decode tests remain. Removing the generated names breaks both new gates.

`crucible-core/src/protocol/session_events/mod.rs:133` manually maps wire names
to groups before deserializing them into the payload enums. The completeness
tests at `session_events/tests.rs:230` and `:255` scan Rust source to extract
variant names. This duplicates declarations and ties correctness checks to
source syntax, contrary to the repository's exhaustive-table guidance.

Derive variant metadata, or use a small shared declaration that produces the
wire-name routing and enum metadata together. Preserve serde's existing names
and the distinction between malformed known events and unknown future events.
Keep actual serialization round trips and recorded-fixture decoding as the
compatibility tests. Do not replace client projections with one enormous enum
that also owns their presentation behavior.

**Gates:** adding a variant automatically supplies routing or fails compilation;
renaming a wire tag breaks a wire fixture; unknown future events retain their
forward-compatible path; malformed known events remain distinguishable.

## 6. Make observer delivery policy explicit before broadening the bus

`server/file_event_hooks.rs:48` drops lagged events on the rationale that the
next change will retrigger the observer. Its dispatch table now also contains
webhooks and session lifecycle events, which need not recur.
`session_lifecycle.rs:523` deliberately runs scoped end observers directly
before removing their registrations, then broadcasts for global observers.

Do not remove that direct lifecycle path merely to make every arrow pass through
the bus. First declare which observers are coalescible invalidations and which
need completion before cleanup. If the bus is to own the latter, it needs an
explicit completion barrier and cancellation/error behavior. Webhook delivery
guarantees likewise need a policy decision, not an unbounded queue by default.

**Gates:** a slow handler and a tiny ring expose loss behavior; scoped end
observers run once before cleanup; global end observers have a stated delivery
contract; handler failure cannot deadlock session teardown.

## Suggested order

1. Move publications onto the existing system root and share proposal
   invalidation rules. These are the smallest reviewable reductions.
2. Consolidate gap/reconnect handling and broker subscription ownership, with
   real socket/SSE tests first.
3. Introduce the owned daemon bus and derived event metadata as separate
   refactors, preserving the current wire format.
4. Revisit observer delivery guarantees separately from transport cleanup.

The recent notification reconciliation should remain a concrete session-owned
consumer for now. Interactions have another snapshot race, but their optimistic
answer rollback and temporary suppression map (`web/src/lib/query/interactions.ts`)
are different semantics. A generic reducer framework would add abstraction
before there is a sufficiently shared contract.
