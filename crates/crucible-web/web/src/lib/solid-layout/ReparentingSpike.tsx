/**
 * Reparenting Spike — validates SolidJS reactive state survives DOM
 * reparenting via appendChild (mirrors VanillaLayoutRenderer pattern).
 */

import { createSignal, onCleanup, onMount } from "solid-js";
import { render } from "solid-js/web";

function Counter() {
  const [count, setCount] = createSignal(0);
  const [cleanupCalled, setCleanupCalled] = createSignal(false);

  const interval = setInterval(() => {
    setCount((c) => c + 1);
  }, 100);

  onCleanup(() => {
    clearInterval(interval);
    setCleanupCalled(true);
    const el = document.querySelector("[data-cleanup-called]");
    if (el) el.setAttribute("data-cleanup-called", "true");
  });

  return (
    <div class="counter-component" style="padding: 16px; font-family: monospace;">
      <div class="counter" data-testid="counter-value">
        Count: {count()}
      </div>
      <div
        class="cleanup-status"
        data-testid="cleanup-status"
        data-cleanup-called={cleanupCalled() ? "true" : "false"}
      >
        Cleanup called: {cleanupCalled() ? "YES (component destroyed!)" : "no (good)"}
      </div>
    </div>
  );
}

export function ReparentingSpike() {
  let tabsetA: HTMLDivElement | undefined;
  let tabsetB: HTMLDivElement | undefined;
  let contentContainer: HTMLDivElement | undefined;
  let disposeFn: (() => void) | undefined;

  const [currentParent, setCurrentParent] = createSignal<"A" | "B">("A");
  const [moveCount, setMoveCount] = createSignal(0);

  onMount(() => {
    if (!tabsetA) return;

    contentContainer = document.createElement("div");
    contentContainer.className = "tab-content-container";
    contentContainer.style.cssText = "border: 2px solid #4a9eff; padding: 8px; border-radius: 4px;";

    disposeFn = render(() => <Counter />, contentContainer);
    tabsetA.appendChild(contentContainer);
  });

  onCleanup(() => {
    disposeFn?.();
  });

  function moveTab() {
    if (!contentContainer || !tabsetA || !tabsetB) return;

    if (currentParent() === "A") {
      tabsetB.appendChild(contentContainer);
      setCurrentParent("B");
    } else {
      tabsetA.appendChild(contentContainer);
      setCurrentParent("A");
    }
    setMoveCount((c) => c + 1);
  }

  return (
    <div style="padding: 24px; font-family: sans-serif; color: #e0e0e0; background: #1a1a2e; min-height: 100vh;">
      <h1 style="margin-top: 0;">SolidJS Reparenting Spike</h1>
      <p style="color: #8899aa;">
        Tests whether SolidJS reactive state survives DOM reparenting
        (moving a container div between parent elements via appendChild).
      </p>

      <div style="margin-bottom: 16px; display: flex; gap: 16px; align-items: center;">
        <button
          data-testid="move-button"
          onClick={moveTab}
          style="padding: 8px 16px; background: #4a9eff; color: white; border: none; border-radius: 4px; cursor: pointer; font-size: 14px;"
        >
          Move Tab: {currentParent()} → {currentParent() === "A" ? "B" : "A"}
        </button>
        <span data-testid="move-count" style="color: #8899aa;">
          Moves: {moveCount()}
        </span>
        <span data-testid="current-parent" style="color: #8899aa;">
          Current parent: TabSet {currentParent()}
        </span>
      </div>

      <div style="display: grid; grid-template-columns: 1fr 1fr; gap: 16px;">
        <div style="border: 2px solid #333; border-radius: 8px; overflow: hidden;">
          <div style="background: #333; padding: 8px 12px; font-weight: bold;">
            TabSet A
          </div>
          <div
            ref={tabsetA}
            data-testid="tabset-a"
            style="min-height: 120px; padding: 8px; background: #222;"
          />
        </div>

        <div style="border: 2px solid #333; border-radius: 8px; overflow: hidden;">
          <div style="background: #333; padding: 8px 12px; font-weight: bold;">
            TabSet B
          </div>
          <div
            ref={tabsetB}
            data-testid="tabset-b"
            style="min-height: 120px; padding: 8px; background: #222;"
          />
        </div>
      </div>

      <div style="margin-top: 24px; padding: 16px; background: #222; border-radius: 8px; font-size: 13px; color: #8899aa;">
        <strong style="color: #e0e0e0;">How this spike works:</strong>
        <ol style="margin: 8px 0 0 0; padding-left: 20px;">
          <li>A SolidJS Counter component is rendered via <code>render()</code> into an imperative container div</li>
          <li>The counter increments every 100ms via setInterval</li>
          <li>Click "Move Tab" to reparent the container div from one tabset to another via <code>appendChild</code></li>
          <li>If the counter value is preserved (not reset to 0), reparenting works!</li>
          <li>If "Cleanup called: YES" appears, the component was destroyed</li>
        </ol>
      </div>
    </div>
  );
}
