/**
 * Marks the synchronous window in which a persisted layout is being applied
 * over the live store. Solid flushes effects before `setStore` returns, so a
 * plain flag with try/finally brackets every effect that reacts to the
 * restore — EdgePanel uses it to SNAP panels to their restored state instead
 * of tweening: a restore is initialization, not an interaction, and playing
 * a 200ms slide on page load both looks wrong and moves the center layout
 * under anything measuring it (real bug: e2e drags grabbed stale tab
 * coordinates mid-slide).
 */
let restoring = false;

export function markLayoutRestore<T>(apply: () => T): T {
  restoring = true;
  try {
    return apply();
  } finally {
    restoring = false;
  }
}

export function isRestoringLayout(): boolean {
  return restoring;
}
