# Reparenting Spike Results

**Date**: 2026-02-10
**Question**: Does SolidJS reactive state survive DOM reparenting via `appendChild`?
**Answer**: **YES** — state fully preserved, no workarounds needed.

## Result: Pattern A (Ideal)

Content DOM **survives the move** without destruction, state loss, or flicker.

## Evidence

### Playwright Test (automated)
- Test: `e2e/reparenting-spike.spec.ts`
- Result: **PASS** (1.9s)
- Counter value before move: >0 (accumulated via `setInterval`)
- Counter value after move: **continued incrementing** (not reset to 0)
- `onCleanup` callback: **never fired** (component not destroyed)
- After round-trip (A→B→A): counter still incrementing, no cleanup

### Manual Verification (Playwright MCP browser)
- Counter at ~52 in TabSet A
- Clicked "Move Tab" → counter moved to TabSet B, value continued from ~178
- After 1s in TabSet B → counter at ~237
- `data-cleanup-called` attribute remained `"false"` throughout
- Screenshot: `.sisyphus/evidence/task-0-reparenting-spike.png`

## The Pattern

SolidJS components rendered into imperative container divs via `render()` survive
DOM reparenting because SolidJS tracks reactivity through its signal/effect system,
not through DOM tree position. Moving a DOM node via `appendChild` is transparent
to Solid's reactive runtime.

```typescript
// 1. Create imperative container
const container = document.createElement("div");

// 2. Render SolidJS component into it
const dispose = render(() => <MyComponent />, container);

// 3. Mount into parent A
parentA.appendChild(container);

// 4. Reparent to parent B — state survives!
parentB.appendChild(container);

// 5. Dispose only when truly destroying the component
// dispose();
```

This is exactly what `VanillaLayoutRenderer` + `SolidContentRenderer` already do:
- `VanillaLayoutRenderer` manages a `contentContainers` Map (tabId → HTMLDivElement)
- `SolidContentRenderer.init()` calls `render()` into the container
- When tabs move between tabsets, `appendChild` reparents the container
- SolidJS reactivity continues working inside the moved container

## Why It Works

1. **SolidJS reactivity is signal-based, not tree-based**: Signals, effects, and
   computations are tracked in a global reactive graph. DOM nodes are just output
   targets — their position in the DOM tree doesn't matter.

2. **`appendChild` moves, not copies**: The DOM spec says `appendChild` on an
   already-attached node removes it from its current parent and attaches to the
   new one. No destruction/recreation occurs.

3. **No `onCleanup` triggered**: SolidJS `onCleanup` is tied to the owner scope
   (created by `render()`), not to DOM attachment. Moving the DOM node doesn't
   dispose the owner scope.

## What This Means for the Rewrite

**No Portal, ref pool, or workaround pattern needed.** The existing architecture
(VanillaLayoutRenderer + SolidContentRenderer) already handles reparenting correctly.

The SolidJS rewrite can proceed with confidence:
- Tab content components rendered via `render()` into imperative containers
- Layout engine manages container position via `appendChild`
- SolidJS reactive state (signals, stores, effects, intervals, subscriptions) all survive
- `onCleanup` only fires when `dispose()` is explicitly called (tab close/delete)

## Decision Gate

**PROCEED** with the full rewrite. The critical architectural question is resolved
favorably — SolidJS content reparenting works out of the box with the imperative
container pattern.
