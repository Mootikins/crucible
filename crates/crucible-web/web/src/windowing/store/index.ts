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

/** Replace every field of the store with the fields of `next`. */
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
 * Test-only. The page configures the core once and never resets it; a test
 * calls this in `beforeEach` to start each case from an unconfigured core.
 */
export function resetWindowingForTest(): void {
  configured = null;
  replaceState(emptyState());
}

export type WindowActions = TabActions & LayoutActions & FloatingWindowActions;

const context = { store, setStore };
const actions: WindowActions = {
  ...createTabActions(context, policy),
  ...createLayoutActions(context, policy),
  ...createFloatingWindowActions(context),
};

/**
 * Wrap each action so that a call before `configureWindowing` throws.
 *
 * An action that finds nothing to do returns before it reads the policy, so
 * without this guard an early call would pass in silence. The wrappers are
 * cached, so one action keeps one identity.
 */
const guarded = new Map<PropertyKey, unknown>();
export const windowActions: WindowActions = new Proxy(actions, {
  get(target, key, receiver) {
    const value: unknown = Reflect.get(target, key, receiver);
    if (typeof value !== 'function') return value;
    let wrapper = guarded.get(key);
    if (!wrapper) {
      wrapper = (...args: unknown[]) => {
        policy();
        return Reflect.apply(value, target, args);
      };
      guarded.set(key, wrapper);
    }
    return wrapper;
  },
});

export { store as windowStore, setStore };

/** The rail that holds the group, or null for a centre or floating group. */
export function findEdgePanelForGroup(groupId: string) {
  return findInState(store, groupId);
}
