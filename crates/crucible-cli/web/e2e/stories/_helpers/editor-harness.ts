import type { Page, Route } from '@playwright/test';
import { waitForFonts } from './fonts';

declare global {
  interface Window {
    __editorHarness?: {
      open: (path: string) => Promise<void>;
      save: (path: string) => Promise<void>;
      activePath: () => string | null;
    };
  }
}

/**
 * Mock wiring for the dev-only editor harness (/editor-harness.html), which
 * mounts the real EditorProvider + EditorPanel. See
 * src/test-harness/editor-harness.tsx.
 */
export const HARNESS_KILN = '/home/user/.crucible/kiln';

export interface HarnessFile {
  /** Note name as the app addresses it (path minus kiln prefix, minus .md). */
  name: string;
  /** Absolute path, e.g. `${HARNESS_KILN}/Note A.md`. */
  path: string;
  content: string;
}

export interface SavedNote {
  path: string;
  content: string;
}

export interface EditorHarness {
  /** PUT bodies captured from the real saveNote() path, in order. */
  saves: SavedNote[];
  /** Open a file through the real EditorContext.openFile. */
  open(file: HarnessFile): Promise<void>;
}

/**
 * Route the note endpoints and navigate to the harness. GET returns the seeded
 * content; PUT records the save and 200s (like a successful daemon write).
 */
export interface HarnessOptions {
  /** Dock the real BacklinksPanel beside the editor (`?backlinks=1`). */
  backlinks?: boolean;
  /** Keep the product default (vim ON) instead of the test default (off). */
  vim?: boolean;
}

export async function setupEditorHarness(
  page: Page,
  files: HarnessFile[],
  options: HarnessOptions = {},
): Promise<EditorHarness> {
  const project = {
    path: '/home/user/project',
    name: 'project',
    kilns: [{ path: HARNESS_KILN, name: 'My Kiln' }],
    last_accessed: '2026-01-01T00:00:00Z',
  };
  const saves: SavedNote[] = [];
  const byPath = new Map(files.map((f) => [f.path, f]));

  await page.route('**/api/project/list', (r) => r.fulfill({ json: [project] }));

  // Editor load + save both go through /api/kiln/file by absolute path:
  //   GET  → return the seeded bytes (get_note_by_name has no content)
  //   PUT  → record the save { path, content } and 200
  await page.route('**/api/kiln/file**', (route: Route) => {
    const req = route.request();
    if (req.method() === 'GET') {
      const p = new URL(req.url()).searchParams.get('path') ?? '';
      const file = byPath.get(p);
      if (!file) return route.fulfill({ status: 404, body: 'not found' });
      return route.fulfill({ json: { content: file.content } });
    }
    if (req.method() === 'PUT') {
      const body = req.postDataJSON() as { path: string; content: string };
      saves.push({ path: body.path, content: body.content });
      return route.fulfill({ status: 200, body: '' });
    }
    return route.continue();
  });

  // Vim keybindings default ON in the product; most stories type plain text,
  // so they run with vim disabled via the persisted setting. The vim story
  // passes { vim: true } to exercise the real default.
  if (!options.vim) {
    await page.addInitScript(() => {
      localStorage.setItem('crucible:settings', JSON.stringify({ editor: { vimMode: false } }));
    });
  }

  await page.goto(options.backlinks ? '/editor-harness.html?backlinks=1' : '/editor-harness.html');
  await page.waitForFunction(() => Boolean(window.__editorHarness));
  // Guard visual baselines against FOUT (fallback-font capture in the dev-server).
  await waitForFonts(page);

  return {
    saves,
    async open(file: HarnessFile) {
      await page.evaluate((p) => window.__editorHarness!.open(p), file.path);
    },
  };
}

/** Append text at the end of the active CodeMirror document. */
export async function typeInEditor(page: Page, text: string): Promise<void> {
  await page.locator('.cm-content').first().click();
  // Jump to end of document so appended text does not land at the click point.
  await page.keyboard.press('ControlOrMeta+End');
  await page.keyboard.type(text);
}
