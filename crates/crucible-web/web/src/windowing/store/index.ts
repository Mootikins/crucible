import { createStore, produce } from 'solid-js/store';
import type { WindowState } from '../model/types';
import { emptyState, findEdgePanelForGroup as findInState } from '../model/tree';
import type { WindowPolicy } from './policy';
import { createTabActions, type TabActions } from './tabActions';
import { createLayoutActions, type LayoutActions } from './layoutActions';
import {
  createFloatingWindowActions,
  type FloatingWindowActions,
} from './floatingWindowActions';

export type { WindowPolicy } from './policy';

const [store, setStore] = createStore<WindowState>(emptyState());
let configured: WindowPolicy | null = null;

/** The policy in force. Throws before `configureWindowing` ran. */
export function policy(): WindowPolicy {
  if (!configured) throw new Error('windowing: call configureWindowing(policy) before any action');
  return configured;
}

/**
 * Replace every field of the store with the fields of `next`.
 *
 * `Object.assign` merges. Every key of `WindowState` is required, so the merge
 * replaces the whole state. A future optional key needs an explicit delete.
 */
function replaceState(next: WindowState): void {
  setStore(produce((s) => Object.assign(s, next)));
}

/**
 * Give the core its decisions and its seed. Call it once per page: the app
 * calls it in stores/windowStore.ts, and the harness calls it in its own entry.
 */
export function configureWindowing<C extends string>(next: WindowPolicy<C>): void {
  // No cast: the policy members use method syntax, which TypeScript checks
  // bivariantly, so a WindowPolicy<C> is a WindowPolicy<string>.
  configured = next;
  replaceState(next.seed());
}

/**
 * Forget the policy and empty the store.
 *
 * @internal Test-only. The page configures the core once and never resets it; a test
 * calls this in `beforeEach` to start each case from an unconfigured core.
 */
export function resetWindowingForTest(): void {
  configured = null;
  replaceState(emptyState());
}

export type WindowActions<C extends string = string> = TabActions<C> &
  LayoutActions<C> &
  FloatingWindowActions;

/**
 * Wrap each action so that a call before `configureWindowing` throws.
 *
 * Most actions never read the policy. Without the wrapper, such an action
 * changes the store before configuration, and `configureWindowing` then
 * erases that change in silence. The wrappers are plain properties, so
 * `vi.spyOn` replaces them like any other method.
 */
function requirePolicy<T extends object>(actions: T): T {
  const wrapped = Object.entries(actions).map(([key, action]: [string, unknown]) => [
    key,
    typeof action === 'function'
      ? (...args: unknown[]) => {
          policy();
          return Reflect.apply(action, actions, args);
        }
      : action,
  ]);
  // Object.fromEntries loses the key types. Each key keeps its name, and each
  // wrapper takes and returns what its action does, so the shape is still T.
  return Object.fromEntries(wrapped) as T;
}

const context = { store, setStore };
export const windowActions: WindowActions = requirePolicy({
  ...createTabActions(context, policy),
  ...createLayoutActions(context, policy),
  ...createFloatingWindowActions(context),
});

export { store as windowStore, setStore };

/** The rail that holds the group, or null for a centre or floating group. */
export function findEdgePanelForGroup(groupId: string) {
  return findInState(store, groupId);
}
