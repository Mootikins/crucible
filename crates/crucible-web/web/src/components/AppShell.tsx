import type { Component } from 'solid-js';
import { WindowManager } from '@/windowing';
import { MobileShell } from '@/components/mobile/MobileShell';
import { isCompact } from '@/stores/deviceStore';
import { renderPanel } from '@/lib/render-panel';
import { appWindowSlots } from '@/components/shell/windowSlots';
import { WikilinkHoverPreview } from '@/components/WikilinkHoverPreview';

/**
 * The one shell this page draws. Never both: the two shells hold different tab
 * stores, and two live editors on one file would each claim its dirty state.
 * `isCompact()` is fixed at load, so this choice never flips under a user.
 *
 * The window manager knows no app. The app gives it the panel renderer and
 * hangs its chrome on the slots.
 */
export const AppShell: Component = () =>
  isCompact() ? (
    <MobileShell />
  ) : (
    <WindowManager
      renderContent={renderPanel}
      slots={appWindowSlots}
    >
      {/* Inside the drag provider, so a hover card can drag a file tab into
          the window system. */}
      <WikilinkHoverPreview />
    </WindowManager>
  );
