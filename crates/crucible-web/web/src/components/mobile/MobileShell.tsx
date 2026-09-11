import { Component, createSignal } from 'solid-js';
import { ContentSurface } from '@/components/mobile/ContentSurface';
import type { Tab } from '@/types/windowTypes';

/**
 * The compact shell: an app bar over one content surface.
 *
 * It shares every panel, context and store with the desktop shell except the
 * layout. See `docs/Meta/Architecture/Mobile Shell.md`. The drawers, the tab
 * stack and the empty state's actions arrive in later steps of Track A.
 */
export const MobileShell: Component = () => {
  // Replaced by the tab stack in step 3; until then nothing opens a tab.
  const [activeTab] = createSignal<Tab | null>(null);

  return (
    <div
      class="flex flex-col h-dvh bg-shell-bg text-shell-ink overflow-hidden"
      data-testid="mobile-shell"
    >
      <header
        class="shrink-0 flex items-center h-12 px-3 border-b border-hairline bg-surface-base"
        style={{ 'padding-top': 'var(--inset-top)', 'box-sizing': 'content-box' }}
      >
        <h1 class="flex-1 truncate text-sm font-medium text-shell-ink">
          {activeTab()?.title ?? 'Crucible'}
        </h1>
      </header>
      <main
        class="flex-1 min-h-0 flex flex-col"
        style={{ 'padding-bottom': 'var(--inset-bottom)' }}
      >
        <ContentSurface
          tab={activeTab}
          empty={
            <div class="flex-1 flex items-center justify-center px-6">
              <p class="text-muted-dark text-sm">No note is open.</p>
            </div>
          }
        />
      </main>
    </div>
  );
};
