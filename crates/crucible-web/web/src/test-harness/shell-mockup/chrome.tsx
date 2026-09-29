/**
 * The chrome on the rails: the controls of the old top bar, in the ribbon's
 * head and tail slots, where the current app keeps its own.
 */
import { For, Show, createSignal, type Component, type JSX } from 'solid-js';
import { Portal } from 'solid-js/web';
import { ArrowLeftRight, Bell, Contrast, Plus, Search, Settings, SlidersHorizontal } from 'lucide-solid';
import { RibbonCommand } from '@/windowing/components/RibbonButton';
import type { WindowingSlots } from '@/windowing/components/context';
import type { EdgePanelPosition } from '@/windowing/model/types';
import { windowActions, windowStore } from '@/windowing/store';
import { answerPermission, pendingHunks, setState, state } from './state';
import { Toolbox, setTweak, toggleToolbox, tweaks } from './toolbox';
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
          <RibbonCommand title="Switch theme" testId="mk-theme" onClick={() => setTweak('theme', tweaks.theme === 'dark' ? 'light' : 'dark')}><Contrast class="w-4 h-4" /></RibbonCommand>
          <RibbonCommand title="Look" testId="mk-look" onClick={toggleToolbox}><SlidersHorizontal class="w-4 h-4" /></RibbonCommand>
          {/* The swap button: it swaps what opens in the centre, and moves
              nothing. The core's own swap button (it swaps the rails) is
              hidden by the mockup stylesheet. */}
          <button
            type="button"
            class="mk-railbtn"
            data-testid="mk-spawn"
            aria-pressed={state.spawn === 'sessions'}
            title={state.spawn === 'docs' ? 'Documents open in the centre. Switch: sessions open there' : 'Sessions open in the centre. Switch: documents open there'}
            onClick={() => setState('spawn', state.spawn === 'docs' ? 'sessions' : 'docs')}
          >
            <ArrowLeftRight class="w-4 h-4" />
          </button>
          <button type="button" class="mk-railbtn" title="Settings" onClick={(e) => openPop('settings', e)}><Settings class="w-4 h-4" /></button>
          <Popover kind="inbox"><InboxBody /></Popover>
          <Popover kind="settings"><SettingsBody /></Popover>
          <Toolbox />
        </>
      ) : undefined,
  };
}
