import type { Page, Route } from '@playwright/test';

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
  name: string;
  kiln: string;
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
export async function setupEditorHarness(
  page: Page,
  files: HarnessFile[],
): Promise<EditorHarness> {
  const project = {
    path: '/home/user/project',
    name: 'project',
    kilns: [{ path: HARNESS_KILN, name: 'My Kiln' }],
    last_accessed: '2026-01-01T00:00:00Z',
  };
  const byName = new Map(files.map((f) => [f.name, f]));
  const saves: SavedNote[] = [];

  await page.route('**/api/project/list', (r) => r.fulfill({ json: [project] }));

  await page.route('**/api/notes/**', (route: Route) => {
    const req = route.request();
    const url = new URL(req.url());
    // /api/notes/<encoded name>?kiln=...
    const encoded = url.pathname.replace(/^\/api\/notes\//, '');
    const name = decodeURIComponent(encoded);

    if (req.method() === 'PUT') {
      const body = req.postDataJSON() as { kiln: string; content: string };
      saves.push({ name, kiln: body.kiln, content: body.content });
      return route.fulfill({ status: 200, body: '' });
    }

    const file = byName.get(name);
    if (!file) return route.fulfill({ status: 404, body: 'not found' });
    return route.fulfill({
      json: { name, content: file.content, path: file.path },
    });
  });

  await page.goto('/editor-harness.html');
  await page.waitForFunction(() => Boolean(window.__editorHarness));

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
