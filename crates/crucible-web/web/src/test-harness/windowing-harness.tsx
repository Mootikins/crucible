/**
 * DEV/TEST-ONLY windowing harness. It is NOT part of the shipped app.
 *
 * Vite serves it in dev at `/windowing-harness.html`. The production rollup
 * input does not name it, so it never reaches `dist/`. The core specs in
 * `e2e/windowing/` open this page to drive the window manager with no app:
 * no panel registry, no rails rule, no server.
 *
 * The page imports nothing from the app except `@/index.css`, which gives the
 * tokens and the fonts. The policy is the neutral policy that the core's unit
 * tests use. The page exposes `__windowStore` and `__windowActions` so a spec
 * can read and change the store.
 */
import '@/index.css';
import { render } from 'solid-js/web';
import { configureWindowing, windowStore, windowActions } from '@/windowing/store';
import { WindowManager } from '@/windowing/components/WindowManager';
import { neutralPolicy } from '@/windowing/testing/neutralPolicy';

configureWindowing(neutralPolicy());
Object.assign(window, { __windowStore: windowStore, __windowActions: windowActions });

render(
  () => (
    <WindowManager
      renderContent={(tab) => (
        <div
          class="flex-1 flex items-center justify-center"
          data-testid={`content-${tab().contentType}`}
        >
          {tab().title}
        </div>
      )}
      slots={{}}
    />
  ),
  document.getElementById('root')!,
);
