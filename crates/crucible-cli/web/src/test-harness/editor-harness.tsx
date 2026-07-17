/**
 * DEV/TEST-ONLY editor harness — NOT part of the shipped app.
 *
 * Served by Vite in dev at `/editor-harness.html`; it is not in the production
 * rollup input, so it never lands in `dist/`. Playwright story specs
 * (e2e/stories/editor-*.story.spec.ts) navigate here to drive the REAL editor
 * components — `EditorProvider`, `EditorPanel`, `FileViewerPanel` — in a real
 * browser with video/screenshots, without the registry-bypass antipattern used
 * by e2e/file-tab.spec.ts.
 *
 * Why a harness instead of the app's own file flow: as of this writing the app
 * (src/App.tsx) never mounts <EditorProvider>, and no UI element calls
 * EditorContext.saveFile — so the editor is unreachable and unsaveable through
 * the running app. See docs/Meta/Web User Stories.md (WS-202/204 GAP notes) and
 * the spec headers. This harness mounts the same real components under the real
 * providers and adds the one missing affordance (a Save button wired to the
 * real saveFile) so the genuine round-trip code path is exercised end-to-end.
 */
import '@/index.css';
import { render } from 'solid-js/web';
import { Show, For, onMount, onCleanup, type Component } from 'solid-js';
import { DragDropProvider, DragDropSensors } from '@thisbeyond/solid-dnd';
import { ProjectProvider } from '@/contexts/ProjectContext';
import { SettingsProvider } from '@/contexts/SettingsContext';
import { EditorProvider, useEditor } from '@/contexts/EditorContext';
import { EditorPanel } from '@/components/EditorPanel';
import { BacklinksPanel } from '@/components/BacklinksPanel';
import { WikilinkHoverPreview } from '@/components/WikilinkHoverPreview';
import { FloatingWindow } from '@/components/windowing/FloatingWindow';
import { windowStore } from '@/stores/windowStore';
import { registerPanels } from '@/lib/register-panels';

// Floating windows resolve their content through the panel registry — the
// hover popover's 'file' tab needs FileViewerPanel registered here too.
registerPanels();

// `?backlinks=1` docks the real BacklinksPanel beside the editor so the
// backlinks/wikilink story specs can drive it against the real EditorContext.
const withBacklinks = () =>
  new URLSearchParams(window.location.search).get('backlinks') === '1';

interface HarnessApi {
  open: (path: string) => Promise<void>;
  save: (path: string) => Promise<void>;
  activePath: () => string | null;
}

declare global {
  interface Window {
    __editorHarness?: HarnessApi;
  }
}

const HarnessInner: Component = () => {
  const editor = useEditor();

  // Publish an imperative handle so specs can open/save without depending on a
  // product "open file" affordance (which the app currently lacks).
  window.__editorHarness = {
    open: (path: string) => editor.openFile(path),
    save: (path: string) => editor.saveFile(path),
    activePath: () => editor.activeFile(),
  };

  const saveActive = () => {
    const p = editor.activeFile();
    if (p) void editor.saveFile(p);
  };

  // The app routes crucible:open-file to its window-tab editor (App.tsx);
  // here the same event opens the file as an EditorPanel tab, so panels that
  // dispatch it (BacklinksPanel linked mentions) work under the harness.
  onMount(() => {
    const onOpenFile = (e: Event) => {
      const { path } = (e as CustomEvent<{ path: string }>).detail;
      void editor.openFile(path);
    };
    window.addEventListener('crucible:open-file', onOpenFile);
    onCleanup(() => window.removeEventListener('crucible:open-file', onOpenFile));
  });

  return (
    <div class="h-screen flex flex-col bg-neutral-950 text-neutral-100">
      <div class="flex items-center gap-2 border-b border-neutral-800 px-3 py-2">
        <span class="text-xs text-neutral-500">editor-harness</span>
        <button
          data-testid="harness-save"
          onClick={saveActive}
          class="rounded bg-blue-600 px-3 py-1 text-sm hover:bg-blue-500"
        >
          Save
        </button>
        <span data-testid="harness-open-count" class="text-xs text-neutral-500">
          {editor.openFiles().length} open
        </span>
        <Show when={editor.error()}>
          <span data-testid="harness-error" class="text-xs text-red-400">
            {editor.error()}
          </span>
        </Show>
      </div>
      <div class="flex flex-1 overflow-hidden">
        <div class="flex-1 overflow-hidden">
          <EditorPanel />
        </div>
        <Show when={withBacklinks()}>
          <div class="w-80 shrink-0 border-l border-neutral-800" data-testid="harness-backlinks">
            <BacklinksPanel />
          </div>
        </Show>
      </div>
      <WikilinkHoverPreview />
      {/* Hover popovers are transient FloatingWindows — the harness hosts
          the same floating layer as the app so hover stories exercise the
          real window, not a mock. */}
      <div class="fixed inset-0 z-30 pointer-events-none">
        <For each={windowStore.floatingWindows.filter((w) => !w.isMinimized)}>
          {(w) => (
            <div class="pointer-events-auto">
              <FloatingWindow window={w} />
            </div>
          )}
        </For>
      </div>
    </div>
  );
};

const EditorHarness: Component = () => (
  <ProjectProvider>
    {/* Real SettingsProvider so persisted editor settings (vim mode) apply. */}
    <SettingsProvider>
      <EditorProvider>
        {/* FloatingWindow tab bars register solid-dnd droppables. */}
        <DragDropProvider>
          <DragDropSensors>
            <HarnessInner />
          </DragDropSensors>
        </DragDropProvider>
      </EditorProvider>
    </SettingsProvider>
  </ProjectProvider>
);

const root = document.getElementById('root');
if (root) {
  render(() => <EditorHarness />, root);
}
