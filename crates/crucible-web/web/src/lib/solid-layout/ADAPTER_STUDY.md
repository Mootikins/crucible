# Study: Old solid-flexlayout Adapter

## Overview

This document analyzes the abandoned `src/lib/solid-flexlayout/` adapter (~200 LOC) to identify what patterns worked, what failed architecturally, and what lessons apply to the new SolidJS implementation.

**Key Finding**: The old adapter had a critical architectural flaw (IIFE render pattern) that defeated SolidJS's fine-grained reactivity, causing full DOM destruction/recreation on every model change.

---

## Architecture Summary

### File Structure
- **SolidBinding.tsx** (141 LOC) — Main adapter component
- **Layout.tsx** (16 LOC) — Public wrapper
- **LayoutTypes.ts** (40 LOC) — Type definitions
- **index.ts** (7 LOC) — Exports

### Design Pattern
The adapter used a **thin wrapper** approach:
1. `<Layout>` (public API) → `<SolidBinding>` (implementation)
2. `SolidBinding` creates a `VanillaLayoutRenderer` on mount
3. Renderer handles all DOM manipulation (vanilla imperative code)
4. `SolidContentRenderer` bridges tab content rendering to SolidJS

---

## Patterns Used

### 1. Content Rendering Factory Pattern ✓ GOOD
```typescript
// From SolidBinding.tsx:52
this.disposeFn = render(() => this.factory(this.params!.node), this.container!);
```

**What it did right**:
- Accepted a `factory: (node: TabNode) => JSX.Element` function from props
- Called `render()` to mount SolidJS components into a container
- Stored the dispose function for cleanup
- Allowed parent to control what content renders in each tab

**Why it worked**:
- Decoupled content rendering from layout structure
- Factory pattern is flexible — parent decides what to render
- Proper lifecycle: init → update → dispose

**Lesson for new implementation**: 
✓ **PRESERVE THIS PATTERN** — Use the same factory-based content rendering in new `<Tab>` component

---

### 2. Lifecycle Hook Integration ✓ GOOD
```typescript
// From SolidBinding.tsx:113-137
onMount(() => {
    renderer = new VanillaLayoutRenderer({...});
    renderer.mount(containerRef);
});

onCleanup(() => {
    renderer?.unmount();
    renderer = undefined;
});
```

**What it did right**:
- Used SolidJS `onMount` to initialize the renderer after DOM is ready
- Used `onCleanup` to properly tear down resources
- Stored renderer reference in component scope
- Passed all callbacks (onModelChange, onAction, etc.) to renderer

**Why it worked**:
- Lifecycle hooks are the correct way to manage external resources in SolidJS
- Cleanup prevents memory leaks
- Callbacks allow parent to react to model changes

**Lesson for new implementation**:
✓ **PRESERVE THIS PATTERN** — Use `onMount`/`onCleanup` for resource management in Layout component

---

### 3. Error Boundary Pattern ✓ GOOD
```typescript
// From SolidBinding.tsx:50-57
try {
    this.error = undefined;
    this.disposeFn = render(() => this.factory(this.params!.node), this.container!);
} catch (err) {
    this.error = err instanceof Error ? err : new Error(String(err));
    console.error("Content render error:", this.error);
    this.showErrorFallback();
}
```

**What it did right**:
- Caught rendering errors from the factory function
- Displayed a user-friendly error UI with retry button
- Prevented the entire layout from crashing
- Logged errors for debugging

**Why it worked**:
- Graceful degradation — layout stays functional even if one tab fails
- User can retry without reloading
- Error messages help debugging

**Lesson for new implementation**:
✓ **PRESERVE THIS PATTERN** — Add error boundaries to Tab content rendering

---

## What Failed: The IIFE Render Pattern ✗ CRITICAL FLAW

### The Problem
The old adapter delegated ALL rendering to `VanillaLayoutRenderer`, which is imperative vanilla DOM code. This meant:

1. **No SolidJS reactivity** — The renderer didn't use SolidJS signals or stores
2. **Full DOM recreation on model change** — Every `model.doAction()` triggered a complete re-render
3. **Lost component state** — Any state in rendered components was destroyed and recreated
4. **Performance penalty** — Unnecessary DOM churn, no fine-grained updates

### Why This Happened
The adapter was a **thin wrapper** around existing vanilla code. It didn't rewrite the rendering layer — it just mounted the vanilla renderer into a SolidJS component. This is a common pattern when integrating legacy code, but it defeats the purpose of using SolidJS.

### Evidence
Looking at `SolidBinding.tsx`:
- Line 118-129: Creates `VanillaLayoutRenderer` with all the vanilla rendering logic
- The renderer is created ONCE on mount, then never updated
- Model changes trigger `renderer.doAction()`, which imperatively updates the DOM
- No SolidJS store, no signals, no reactive updates

This is NOT an IIFE pattern in the code itself, but the **effect** is the same: the entire layout tree is re-executed on every model change because the vanilla renderer doesn't know about SolidJS's fine-grained reactivity.

### The Correct Approach (New Implementation)
Instead of wrapping vanilla code, the new implementation should:
1. Create a **reactive store** from `model.toJson()` using SolidJS `reconcile()`
2. Use **SolidJS components** for each layout element (Row, TabSet, Tab, Border, etc.)
3. Let SolidJS handle fine-grained updates — only changed nodes re-render
4. Use **hot-path signals** for high-frequency updates (selectedTabId, dragState)

---

## Integration Patterns Worth Preserving

### 1. Content Rendering Factory
**Current (old adapter)**:
```typescript
interface ILayoutProps {
    factory: (node: TabNode) => JSX.Element;
}
```

**New implementation should**:
- Keep the same factory signature
- Pass it via context to `<Tab>` components
- Use it in `SolidContentRenderer` (or equivalent)

### 2. Lifecycle Management
**Current (old adapter)**:
```typescript
onMount(() => renderer.mount(containerRef));
onCleanup(() => renderer.unmount());
```

**New implementation should**:
- Use `onMount` to initialize the reactive bridge
- Use `onCleanup` to dispose of resources
- Manage ResizeObserver, event listeners, etc.

### 3. Error Handling
**Current (old adapter)**:
```typescript
try {
    this.disposeFn = render(() => this.factory(...), container);
} catch (err) {
    this.showErrorFallback();
}
```

**New implementation should**:
- Wrap content rendering in try-catch
- Show error UI with retry button
- Log errors for debugging

### 4. Props Interface
**Current (old adapter)**:
```typescript
interface ILayoutProps {
    model: Model;
    factory: (node: TabNode) => JSX.Element;
    onAction?: (action: any) => any;
    onModelChange?: (model: Model, action: any) => void;
    onRenderTab?: (node: TabNode, renderValues: ITabRenderValues) => void;
    onRenderTabSet?: (tabSetNode: TabSetNode | BorderNode, renderValues: ITabSetRenderValues) => void;
    onContextMenu?: (node: TabNode, event: MouseEvent) => IMenuItem[];
    onAllowDrop?: (dragNode: Node, dropInfo: any) => boolean;
    classNameMapper?: (defaultClassName: string) => string;
}
```

**New implementation should**:
- Keep the same props interface (or extend it)
- All callbacks should work the same way
- `classNameMapper` should be applied consistently

---

## Lessons for New Implementation

### DO ✓
1. **Use SolidJS reactivity** — Create a store from `model.toJson()`, use `reconcile()` for structural updates
2. **Use hot-path signals** — For high-frequency updates (selectedTabId, dragState), use separate signals
3. **Preserve the factory pattern** — Keep content rendering decoupled via factory function
4. **Use lifecycle hooks** — `onMount`/`onCleanup` for resource management
5. **Add error boundaries** — Catch and display content rendering errors gracefully
6. **Keep the props interface** — Maintain backward compatibility with existing consumers
7. **Apply classNameMapper consistently** — All CSS classes should go through the mapper

### DON'T ✗
1. **Don't wrap vanilla code** — Rewrite rendering as SolidJS components, not wrappers
2. **Don't recreate the entire tree on every update** — Use `reconcile()` for structural diffing
3. **Don't use imperative DOM manipulation** — Let SolidJS handle DOM updates
4. **Don't ignore fine-grained reactivity** — Use signals for state that changes frequently
5. **Don't lose component state** — Ensure content components survive tab selection changes
6. **Don't skip error handling** — Always catch and display rendering errors
7. **Don't change the public API** — Keep `ILayoutProps` compatible

---

## Architectural Recommendations

### 1. Reactive Bridge (Task 1)
Create `src/lib/solid-layout/bridge.ts`:
```typescript
export function createLayoutStore(model: Model) {
    const [store, setStore] = createStore(model.toJson());
    
    // Hook into model's listener
    model.addListener((action) => {
        batch(() => {
            setStore(reconcile(model.toJson(), { key: "id" }));
        });
    });
    
    return store;
}
```

**Why this works**:
- `reconcile()` does structural diffing — only changed nodes update
- `batch()` prevents intermediate renders
- Hot-path signals can update independently for high-frequency changes

### 2. Component Hierarchy
```
<Layout>                          // Root, initializes bridge
  <LayoutContext.Provider>        // Provides store, factory, callbacks
    <BorderLayout>                // Nesting wrapper for borders
      <Border location="left">    // Each border
        <BorderStrip>             // Tab buttons
        <BorderContent>           // Content panel (when selected)
      <Row>                       // Main content area
        <TabSet>                  // Tab container
          <TabBar>                // Tab buttons
          <Tab>                   // Content area
        <Splitter>                // Resize handle
```

**Why this works**:
- Each component is small and focused
- SolidJS can update each component independently
- Content components survive tab selection (no remount)

### 3. Content Rendering
Keep the factory pattern, but use SolidJS `render()`:
```typescript
// In <Tab> component
const [contentDispose, setContentDispose] = createSignal<(() => void) | undefined>();

createEffect(() => {
    const dispose = contentDispose();
    if (dispose) dispose();
    
    const container = tabContentRef;
    if (container && selectedTab()) {
        const newDispose = render(
            () => factory(selectedTab()!),
            container
        );
        setContentDispose(() => newDispose);
    }
});

onCleanup(() => {
    const dispose = contentDispose();
    if (dispose) dispose();
});
```

**Why this works**:
- Content is rendered via SolidJS `render()`, not vanilla DOM
- Dispose function is stored and called on cleanup
- Content survives tab selection changes (container is reused)

---

## Summary

| Aspect | Old Adapter | New Implementation |
|--------|-------------|-------------------|
| **Rendering** | Vanilla DOM (imperative) | SolidJS components (reactive) |
| **Reactivity** | None (full tree re-render) | Fine-grained (only changed nodes) |
| **Content** | Factory pattern ✓ | Factory pattern ✓ |
| **Lifecycle** | onMount/onCleanup ✓ | onMount/onCleanup ✓ |
| **Error handling** | Try-catch + fallback ✓ | Try-catch + fallback ✓ |
| **Performance** | Poor (full DOM churn) | Good (minimal updates) |
| **State preservation** | Lost on update | Preserved (no remount) |

**Key Takeaway**: The old adapter got the **integration patterns right** (factory, lifecycle, error handling) but got the **rendering architecture wrong** (vanilla instead of reactive). The new implementation should preserve the good patterns while fixing the architecture.

---

## References

- **Old adapter**: `src/lib/solid-flexlayout/SolidBinding.tsx:1-140`
- **Vanilla renderer**: `src/lib/flexlayout/rendering/VanillaLayoutRenderer.ts` (5,200 LOC)
- **Model layer**: `src/lib/flexlayout/model/Model.ts` (5,100 LOC, 318 tests)
- **SolidJS reconcile**: https://docs.solidjs.com/concepts/stores#data-integration-with-reconcile
- **SolidJS lifecycle**: https://docs.solidjs.com/reference/component-apis/on-mount
