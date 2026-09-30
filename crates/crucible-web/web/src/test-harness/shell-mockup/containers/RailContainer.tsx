/**
 * The chrome on the rails: the controls of the old top bar, in the ribbon's
 * head and tail slots, where the current app keeps its own.
 */
import { createSignal } from 'solid-js';
import { Contrast, Plus, Search, Settings, SlidersHorizontal } from 'lucide-solid';
import { RibbonCommand } from '@/windowing/components/RibbonButton';
import type { WindowingSlots } from '@/windowing/components/context';
import type { EdgePanelPosition } from '@/windowing/model/types';
import { windowActions, windowStore } from '@/windowing/store';
import { Popover } from '../components/primitives/Popover';
import { InboxButton } from '../components/rail/InboxButton';
import { RailButton } from '../components/rail/RailButton';
import { SettingsPanel } from '../components/rail/SettingsPanel';
import { SpawnButton } from '../components/rail/SpawnButton';
import { setState, state } from '../state';
import { setTweak, toggleToolbox, tweaks } from '../tweaks';
import { InboxContainer, waitingCount } from './InboxContainer';
import { ToolboxContainer } from './ToolboxContainer';

type Pop = 'inbox' | 'settings' | null;
const [pop, setPop] = createSignal<Pop>(null);
const [anchor, setAnchor] = createSignal<DOMRect | null>(null);

/** A second click on the same button closes its popover. */
function openPop(kind: Exclude<Pop, null>, e: MouseEvent) {
  if (pop() === kind) {
    setPop(null);
    return;
  }
  setAnchor((e.currentTarget as HTMLElement).getBoundingClientRect());
  setPop(kind);
}
const closePop = () => setPop(null);

/**
 * Open the settings popover at the rail's settings button, for a command
 * elsewhere ("Manage projects and kilns…" in the Files pane).
 */
export function openSettings() {
  const button = document.querySelector('[data-testid="mk-settings"]');
  if (!button) return;
  setAnchor(button.getBoundingClientRect());
  setPop('settings');
}

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
          <InboxButton count={waitingCount()} onClick={(e) => openPop('inbox', e)} />
          <RibbonCommand title="Switch theme" testId="mk-theme" onClick={() => setTweak('theme', tweaks.theme === 'dark' ? 'light' : 'dark')}><Contrast class="w-4 h-4" /></RibbonCommand>
          <RibbonCommand title="Look" testId="mk-look" onClick={toggleToolbox}><SlidersHorizontal class="w-4 h-4" /></RibbonCommand>
          {/* The core's own swap button (it swaps the rails) is hidden by the
              mockup stylesheet; this one swaps what opens in the centre. */}
          <SpawnButton spawn={state.spawn} onToggle={() => setState('spawn', state.spawn === 'docs' ? 'sessions' : 'docs')} />
          <RailButton title="Settings" testId="mk-settings" onClick={(e) => openPop('settings', e)}><Settings class="w-4 h-4" /></RailButton>
          <Popover open={pop() === 'inbox'} anchor={anchor()} onClose={closePop}>
            <InboxContainer onDone={closePop} />
          </Popover>
          <Popover open={pop() === 'settings'} anchor={anchor()} onClose={closePop}>
            <SettingsPanel
              centreFocusExit={windowStore.expandExit === 'centre-focus'}
              onCentreFocusExit={(on) => windowActions.setExpandExit(on ? 'centre-focus' : 'toggle')}
            />
          </Popover>
          <ToolboxContainer />
        </>
      ) : undefined,
  };
}
