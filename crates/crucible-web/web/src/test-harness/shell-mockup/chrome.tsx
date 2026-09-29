/**
 * The chrome on the rails: the controls of the old top bar, in the ribbon's
 * head and tail slots, where the current app keeps its own.
 */
import { For, Show, createSignal, type Component, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';
import { Bell, Contrast, Plus, Search, Settings } from 'lucide-solid';
import { RibbonCommand } from '@/windowing/components/RibbonButton';
import type { WindowingSlots } from '@/windowing/components/context';
import type { EdgePanelPosition } from '@/windowing/model/types';
import { windowActions, windowStore } from '@/windowing/store';
import { answerPermission, pendingHunks, setState, state } from './state';
import { openChanges } from './actions';

type Pop = 'inbox' | 'settings' | null;
const [pop, setPop] = createSignal<Pop>(null);
const [anchor, setAnchor] = createSignal<DOMRect | null>(null);

function openPop(kind: Exclude<Pop, null>, e: MouseEvent) {
  if (pop() === kind) {
    setPop(null);
    return;
  }
  setAnchor((e.currentTarget as HTMLElement).getBoundingClientRect());
  setPop(kind);
}

/** A popover beside the rail button that opened it. Escape or an outside click closes it. */
const Popover: Component<{ kind: Exclude<Pop, null>; children: JSX.Element }> = (props) => (
  <Show when={pop() === props.kind && anchor()}>
    {(r) => (
      <Portal>
        <div class="mk-scrim" onMouseDown={() => setPop(null)} />
        <div
          class="mk-pop"
          role="dialog"
          style={{ left: `${r().right + 6}px`, bottom: `${Math.max(8, window.innerHeight - r().bottom)}px` }}
          onKeyDown={(e) => e.key === 'Escape' && setPop(null)}
        >
          {props.children}
        </div>
      </Portal>
    )}
  </Show>
);

const waiting = () =>
  Object.keys(state.perms).length +
  Object.keys(state.sessions).filter((sid) => !state.perms[sid] && pendingHunks(sid).length).length;

const PLUGIN_CSS: Record<string, string> = {
  none: '',
  docs: `:root { --cru-color-primary:#3a7fe0; --cru-color-primary-hover:#5e9bf0; --cru-color-primary-active:#2e63b4; --cru-color-on-primary:#0b0f17; --cru-radius-control:0px; --cru-radius-card:2px; --cru-radius-composer:4px; }
:root[data-theme='light'] { --cru-color-primary:#1f4a91; --cru-color-primary-hover:#17376d; --cru-color-primary-active:#102850; --cru-color-on-primary:#ffffff; }`,
};
const [plugin, setPlugin] = createSignal('none');

/**
 * Glass or flat. Glass is the default, except where the OS asks for less
 * transparency: then the flat surfaces stay.
 */
const [material, setMaterialSignal] = createSignal<'glass' | 'flat'>(
  matchMedia('(prefers-reduced-transparency: reduce)').matches ? 'flat' : 'glass',
);
export function applyMaterial(m: 'glass' | 'flat' = material()) {
  setMaterialSignal(m);
  document.documentElement.dataset.material = m;
}
function applyPlugin(id: string) {
  setPlugin(id);
  let el = document.getElementById('mk-plugin-theme');
  if (!el) {
    el = document.createElement('style');
    el.id = 'mk-plugin-theme';
    document.head.append(el);
  }
  el.textContent = PLUGIN_CSS[id] ?? '';
}

function setTheme(theme: 'dark' | 'light') {
  setState('theme', theme);
  // The app's rule: dark writes no attribute.
  if (theme === 'light') document.documentElement.setAttribute('data-theme', 'light');
  else document.documentElement.removeAttribute('data-theme');
}

const InboxBody: Component = () => (
  <div class="mk-inbox">
    <div class="mk-ph">Waiting for you</div>
    <For each={Object.entries(state.perms)}>
      {([sid, p]) => (
        <div class="mk-item">
          <div class="mk-who"><span class="mk-ident" style={{ background: state.sessions[sid]!.color }} />{state.sessions[sid]!.title}</div>
          <div class="mk-quiet">Edit <code>{p.path.split('/').pop()}</code>?</div>
          <div class="mk-acts">
            <button type="button" class="mk-btn sm primary" onClick={() => { answerPermission(sid, 'once'); setPop(null); }}>Allow</button>
            <button type="button" class="mk-btn sm ghost" onClick={() => { answerPermission(sid, 'deny'); setPop(null); }}>Deny</button>
            <button type="button" class="mk-btn sm ghost" onClick={() => { setState('active', sid); setPop(null); }}>Open</button>
          </div>
        </div>
      )}
    </For>
    <For each={Object.keys(state.sessions).filter((sid) => !state.perms[sid] && pendingHunks(sid).length)}>
      {(sid) => (
        <div class="mk-item">
          <div class="mk-who"><span class="mk-ident" style={{ background: state.sessions[sid]!.color }} />{state.sessions[sid]!.title}</div>
          <div class="mk-quiet">{pendingHunks(sid).length} to review</div>
          <div class="mk-acts">
            <button type="button" class="mk-btn sm" onClick={() => { openChanges(sid); setPop(null); }}>Review</button>
          </div>
        </div>
      )}
    </For>
    <Show when={!waiting()}><div class="mk-quiet mk-pad">Nothing waits for you.</div></Show>
  </div>
);

const SettingsBody: Component = () => (
  <div class="mk-settings">
    <div class="mk-ph">Expand</div>
    <label class="mk-check">
      <input
        type="checkbox"
        checked={windowStore.expandExit === 'centre-focus'}
        onChange={(e) => windowActions.setExpandExit(e.currentTarget.checked ? 'centre-focus' : 'toggle')}
      />
      <span>
        End an expanded panel when focus moves to the documents
        <small>Off: only its toggle ends it (Shift+Esc).</small>
      </span>
    </label>
    <div class="mk-ph">Surface</div>
    <For each={[['glass', 'Glass: gradient, blur and grain'], ['flat', 'Flat']] as const}>
      {([id, label]) => (
        <label class="mk-check">
          <input type="radio" name="mk-material" checked={material() === id} onChange={() => applyMaterial(id)} />
          <span>{label}</span>
        </label>
      )}
    </For>
    <div class="mk-ph">Plugin stylesheet</div>
    <For each={[['none', 'None'], ['docs', 'Docs example: blue, square']] as const}>
      {([id, label]) => (
        <label class="mk-check">
          <input type="radio" name="mk-plugin" checked={plugin() === id} onChange={() => applyPlugin(id)} />
          <span>{label}</span>
        </label>
      )}
    </For>
  </div>
);

export function mockSlots(onNewSession: () => void, onSearch: () => void): WindowingSlots {
  return {
    railHead: (position: EdgePanelPosition) =>
      position === 'left' ? (
        <>
          <RibbonCommand title="Search (Ctrl+K)" testId="mk-search" onClick={onSearch}><Search class="w-4 h-4" /></RibbonCommand>
          <RibbonCommand title="New session" testId="mk-new-session" onClick={onNewSession}><Plus class="w-4 h-4" /></RibbonCommand>
        </>
      ) : undefined,
    railTail: (position: EdgePanelPosition) =>
      position === 'left' ? (
        <>
          <button type="button" class="mk-railbtn" title="Inbox" aria-label={`Inbox, ${waiting()} waiting`} onClick={(e) => openPop('inbox', e)}>
            <Bell class="w-4 h-4" />
            <Show when={waiting()}><span class="mk-badge">{waiting()}</span></Show>
          </button>
          <RibbonCommand title="Switch theme" testId="mk-theme" onClick={() => setTheme(state.theme === 'dark' ? 'light' : 'dark')}><Contrast class="w-4 h-4" /></RibbonCommand>
          <button type="button" class="mk-railbtn" title="Settings" onClick={(e) => openPop('settings', e)}><Settings class="w-4 h-4" /></button>
          <Popover kind="inbox"><InboxBody /></Popover>
          <Popover kind="settings"><SettingsBody /></Popover>
        </>
      ) : undefined,
  };
}
