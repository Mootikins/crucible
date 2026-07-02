import { test, expect, request as playwrightRequest } from '@playwright/test';
import { readFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { readState } from './_state';

/**
 * Live tier — real `cru web` + daemon + TempDir kiln. Exercises the kiln/notes
 * endpoints end-to-end (browser/HTTP → daemon → filesystem), the "one kiln
 * truth" the chat and editor surfaces share. Deterministic, no LLM.
 *
 * Skips cleanly when no `cru` binary was available (globalSetup wrote skip).
 */

const state = readState();

test.describe('live kiln truth (WS-201/202/205/206)', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  test('WS-202: a note saved through the browser lands on real disk', async ({ page }) => {
    const baseURL = state.baseURL!;
    const kilnDir = state.kilnDir!;
    const content = '# Browser Save\n\nwritten via the app\n\n- [[Link]] café ☕\n';

    // Drive the save from the loaded app origin (real browser-originated PUT,
    // same request api.ts issues). This is the genuine daemon → FS write path.
    await page.goto(baseURL);
    const status = await page.evaluate(
      async ({ kiln, body }) => {
        const res = await fetch(`/api/notes/${encodeURIComponent('BrowserSave')}`, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ kiln, content: body }),
        });
        return res.status;
      },
      { kiln: kilnDir, body: content },
    );
    expect(status).toBe(200);

    // The file exists on the real filesystem with byte-exact content.
    const onDisk = path.join(kilnDir, 'BrowserSave.md');
    expect(existsSync(onDisk)).toBe(true);
    expect(readFileSync(onDisk, 'utf-8')).toBe(content);
  });

  test('WS-206: a note written via the API is the one shared truth', async () => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const kiln = state.kilnDir!;
    const content = '# Shared\n\nagent and editor see the same bytes\n';

    const put = await api.put(`/api/notes/${encodeURIComponent('Shared')}`, {
      data: { kiln, content },
    });
    expect(put.ok()).toBe(true);

    // Appears in the shared file tree (what the editor/FilesPanel lists).
    const notesRes = await api.get(`/api/kiln/notes?kiln=${encodeURIComponent(kiln)}`);
    expect(notesRes.ok()).toBe(true);
    const notes = (await notesRes.json()) as { files: Array<{ name: string }> };
    expect(notes.files.some((f) => f.name === 'Shared')).toBe(true);

    // The one shared truth is the kiln file on disk — what an agent tool reads
    // next turn. NOTE(finding): GET /api/notes/:name returns metadata only (no
    // `content`), though the frontend's getNote() expects a `content` field —
    // a real backend/frontend mismatch. We assert the on-disk bytes (the truth)
    // and that the metadata read reflects the note.
    const onDisk = path.join(kiln, 'Shared.md');
    expect(readFileSync(onDisk, 'utf-8')).toBe(content);

    const metaRes = await api.get(`/api/notes/${encodeURIComponent('Shared')}?kiln=${encodeURIComponent(kiln)}`);
    expect(metaRes.ok()).toBe(true);
    const meta = (await metaRes.json()) as { title?: string; content?: string };
    expect(meta.title).toBe('Shared');
    expect(meta.content).toBeUndefined(); // documents the missing-content mismatch
    await api.dispose();
  });

  test('WS-201: browse lists notes in the kiln', async () => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const kiln = state.kilnDir!;
    const res = await api.get(`/api/kiln/notes?kiln=${encodeURIComponent(kiln)}`);
    expect(res.ok()).toBe(true);
    const body = (await res.json()) as { files: Array<{ name: string; is_dir: boolean }> };
    // At least the notes created by earlier specs are present.
    expect(body.files.length).toBeGreaterThan(0);
    await api.dispose();
  });

  test('WS-205: a traversal note name is rejected by the daemon', async () => {
    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const kiln = state.kilnDir!;
    const res = await api.put(`/api/notes/${encodeURIComponent('../escape')}`, {
      data: { kiln, content: 'nope' },
    });
    // The route rejects traversal names with a 4xx and never writes outside.
    expect(res.status()).toBeGreaterThanOrEqual(400);
    expect(res.status()).toBeLessThan(500);
    expect(existsSync(path.join(path.dirname(kiln), 'escape.md'))).toBe(false);
    await api.dispose();
  });
});
