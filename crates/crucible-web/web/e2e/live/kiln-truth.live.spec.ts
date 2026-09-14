import { test, expect, request as playwrightRequest } from '@playwright/test';
import { readFileSync, writeFileSync, existsSync } from 'node:fs';
import path from 'node:path';
import { readState } from './_state';
import { appReady } from '../helpers/nav';

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
    // This test waits on the indexing pipeline, so it needs more than the
    // suite's 30s per-test budget (`playwright.live.config.ts`). Without this
    // the poll below CANNOT reach its own timeout: Playwright kills the test
    // at 30s first, the wait reports a generic test-timeout instead of the
    // message the poll carries, and raising the poll budget alone does
    // nothing at all. 90s = the 60s poll plus room for the PUT and the reads.
    test.setTimeout(90_000);

    const api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    const kiln = state.kilnDir!;
    const content = '# Shared\n\nagent and editor see the same bytes\n';

    const put = await api.put(`/api/notes/${encodeURIComponent('Shared')}`, {
      data: { kiln, content },
    });
    expect(put.ok()).toBe(true);

    // Appears in the note index (what wikilink completion lists). The PUT
    // route writes the file and deliberately does NOT upsert the index — the
    // daemon's file watcher runs the note through the pipeline (500ms debounce
    // + processing), so the listing is eventually consistent by design. Poll
    // for that contract instead of racing it.
    //
    // The budget is 60s, not the 15s it was, and the number is measured rather
    // than guessed. Indexing takes ~5.7s on an idle box and ~21.6s while a
    // full `cargo nextest run --workspace` runs beside it. At 15s this test
    // passed alone and failed 4 times out of 4 inside `just ci`, whose own
    // tier supplies exactly that load — so its result tracked how busy the
    // machine was, not whether the note reached the index.
    //
    // 60s is ~3x the loaded measurement. There is no completion signal to wait
    // on instead: the daemon emits `note_created`, but the web layer forwards
    // only `fs_*` events and drops it (`fs_events.rs`,
    // `non_file_events_are_ignored`), and `fs_changed` fires on the WRITE, not
    // on the indexing that follows it. Exposing an index-complete event would
    // let this be deterministic; until then a generous bound is the honest
    // shape. Do not lower it without new measurements.
    //
    // NOTE(finding): even the 60s bound fails INTERMITTENTLY on a quiet box,
    // and the failure is not load: the note never reaches the index at all.
    // Measured 2026-08-25, 30 serialized runs per commit, nothing else
    // running: master dd04e1db0 29/30 pass, the M5 boot inversion 28/30,
    // M5 HEAD 27/30 — Fisher p≈0.30 across the walk, so the rate does not
    // differ by commit; the failure pre-dates M5. Load amplifies it (it
    // fired first inside a `just ci` running beside a full nextest sweep).
    // Mechanism unknown. The open question is whether the daemon emits a
    // file event at all in the failing case — if no `fs_changed` fires, the
    // watcher missed the write; if it fires and the index never updates, the
    // fault is in the debounce or the note pipeline downstream.
    // The live tier runs with retries: 2 (playwright.live.config.ts) FOR
    // THIS finding — a "flaky" line in the run report is this failure
    // firing and is the signal to re-open the investigation, not ordinary
    // CI hygiene to ignore.
    await expect
      .poll(
        async () => {
          const notesRes = await api.get(`/api/kiln/notes?kiln=${encodeURIComponent(kiln)}`);
          if (!notesRes.ok()) return false;
          const notes = (await notesRes.json()) as { files: Array<{ name: string }> };
          return notes.files.some((f) => f.name === 'Shared');
        },
        {
          timeout: 60_000,
          message:
            'the note never reached the kiln index — the daemon watcher, the 500ms debounce, ' +
            'or the note pipeline did not complete within 60s',
        },
      )
      .toBe(true);

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
    // At least the seeded note is present: globalSetup indexed the kiln with
    // `cru process`, so the index-backed listing has content from the start.
    expect(body.files.length).toBeGreaterThan(0);
    expect(body.files.some((f) => f.name === 'Seed')).toBe(true);
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

  // An anchored edit is what makes an offline outbox safe: it names the text it
  // expects, so a stale edit fails loudly instead of overwriting a body someone
  // else changed. This is the only tier where the bytes on disk can be checked.
  test('WS-202: an anchored edit changes its lines and leaves the body byte-exact', async ({ page }) => {
    const baseURL = state.baseURL!;
    const kilnDir = state.kilnDir!;
    const before = '---\nstatus: todo\nupdated: 09-01\n---\n\n# Ticket\n\nA body a human wrote. café ☕\n';
    const notePath = path.join(kilnDir, 'Ticket.md');

    await page.goto(baseURL);
    await page.evaluate(
      async ({ kiln, body }) => {
        await fetch(`/api/notes/${encodeURIComponent('Ticket')}`, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ kiln, content: body }),
        });
      },
      { kiln: kilnDir, body: before },
    );
    await expect.poll(() => existsSync(notePath)).toBe(true);

    const patched = await page.evaluate(async (file) => {
      const res = await fetch('/api/kiln/file', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          path: file,
          edits: [
            { expect: 'status: todo', replace: 'status: doing' },
            { expect: 'updated: 09-01', replace: 'updated: 09-11' },
          ],
        }),
      });
      return { status: res.status, body: await res.json() };
    }, notePath);

    expect(patched.status).toBe(200);
    expect(patched.body.content_hash).toMatch(/^[0-9a-f]{64}$/);
    expect(readFileSync(notePath, 'utf-8')).toBe(
      '---\nstatus: doing\nupdated: 09-11\n---\n\n# Ticket\n\nA body a human wrote. café ☕\n',
    );
  });

  test('WS-202: a stale anchor is refused, whole, and the file is untouched', async ({ page }) => {
    const baseURL = state.baseURL!;
    const kilnDir = state.kilnDir!;
    const before = '---\nstatus: doing\n---\n\n# Stale\n\nuntouched\n';
    const notePath = path.join(kilnDir, 'Stale.md');

    await page.goto(baseURL);
    await page.evaluate(
      async ({ kiln, body }) => {
        await fetch(`/api/notes/${encodeURIComponent('Stale')}`, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ kiln, content: body }),
        });
      },
      { kiln: kilnDir, body: before },
    );
    await expect.poll(() => existsSync(notePath)).toBe(true);

    // The base is the hash on disk now, so the anchor alone refuses the batch.
    // A stale base is refused before any anchor is read (the leg below).
    const refused = await page.evaluate(async (file) => {
      const read = await fetch(`/api/kiln/file?path=${encodeURIComponent(file)}`);
      const { content_hash } = (await read.json()) as { content_hash: string };
      const res = await fetch('/api/kiln/file', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          path: file,
          base_hash: content_hash,
          edits: [
            { expect: 'status: todo', replace: 'status: done' },
            { expect: '# Stale', replace: '# Renamed' },
          ],
        }),
      });
      return { status: res.status, body: await res.json() };
    }, notePath);

    expect(refused.status).toBe(409);
    expect(refused.body.failed).toEqual([{ reason: 'not_found', index: 0 }]);
    expect(refused.body.stale_base).toBe(false);
    expect(refused.body.current_hash).toMatch(/^[0-9a-f]{64}$/);
    // All or nothing: the second edit was fine and must not have landed.
    expect(readFileSync(notePath, 'utf-8')).toBe(before);
  });

  // WS-320: a tick carries the buffer's base. A stale base whose anchor still
  // applies used to write through; the browser then took the answered hash
  // as its base over text that lacked the other writer's paragraph, and its
  // next whole save removed that paragraph with no refusal.
  test('WS-320: a stale base is refused even when its anchors apply', async ({ page }) => {
    const baseURL = state.baseURL!;
    const kilnDir = state.kilnDir!;
    const before = '- [ ] task\n\nagent paragraph\n';
    const notePath = path.join(kilnDir, 'StaleBase.md');

    await page.goto(baseURL);
    await page.evaluate(
      async ({ kiln, body }) => {
        await fetch(`/api/notes/${encodeURIComponent('StaleBase')}`, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ kiln, content: body }),
        });
      },
      { kiln: kilnDir, body: before },
    );
    await expect.poll(() => existsSync(notePath)).toBe(true);

    const refused = await page.evaluate(async (file) => {
      const res = await fetch('/api/kiln/file', {
        method: 'PATCH',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          path: file,
          base_hash: '0'.repeat(64),
          edits: [{ expect: '- [ ] task', replace: '- [x] task' }],
        }),
      });
      return { status: res.status, body: await res.json() };
    }, notePath);

    expect(refused.status).toBe(409);
    expect(refused.body.stale_base).toBe(true);
    expect(refused.body.failed).toEqual([]);
    expect(refused.body.current_hash).toMatch(/^[0-9a-f]{64}$/);
    expect(readFileSync(notePath, 'utf-8')).toBe(before);
  });

  // A client filters and groups notes by their frontmatter. It arrives on the
  // note index, filtered: `scope` is the same-workspace SQL predicate and
  // carries an absolute host path, so it must never cross the wire.
  test('WS-201: the note index carries the author\'s frontmatter, never the daemon\'s stamp', async ({ page }) => {
    const baseURL = state.baseURL!;
    const kilnDir = state.kilnDir!;
    const note = '---\nstatus: doing\nrating: 4\n---\n\n# Props\n';

    await page.goto(baseURL);
    await page.evaluate(
      async ({ kiln, body }) => {
        await fetch(`/api/notes/${encodeURIComponent('Props')}`, {
          method: 'PUT',
          headers: { 'Content-Type': 'application/json' },
          body: JSON.stringify({ kiln, content: body }),
        });
      },
      { kiln: kilnDir, body: note },
    );

    const listed = await expect
      .poll(
        async () =>
          await page.evaluate(async (kiln) => {
            const res = await fetch(`/api/notes?kiln=${encodeURIComponent(kiln)}`);
            const body = await res.json();
            return (body.notes ?? []).find((n: { name: string }) => n.name === 'Props') ?? null;
          }, kilnDir),
        { timeout: 15_000 },
      )
      .not.toBeNull()
      .then(() =>
        page.evaluate(async (kiln) => {
          const res = await fetch(`/api/notes?kiln=${encodeURIComponent(kiln)}`);
          const body = await res.json();
          return (body.notes ?? []).find((n: { name: string }) => n.name === 'Props');
        }, kilnDir),
      );

    expect(listed.properties).toMatchObject({ status: 'doing' });
    expect(listed.properties).not.toHaveProperty('scope');
    expect(JSON.stringify(listed)).not.toContain(kilnDir);
  });

  /**
   * WS-318: two writers, one note, a real daemon and a real disk.
   *
   * This is the rule section 11 states — "the daemon compares the hashes, the
   * browser must never make that decision" — and until now no route could
   * enforce it, so the offline drain compared in the browser instead. The
   * check is only real against a file another process can move under it.
   */
  test('WS-318: a whole-file write whose base moved on is refused, and the disk is untouched', async ({
    page,
  }) => {
    const baseURL = state.baseURL!;
    const kilnDir = state.kilnDir!;
    const notePath = path.join(kilnDir, 'Race.md');

    await page.goto(baseURL);
    const put = (body: string, base?: string) =>
      page.evaluate(
        async ({ file, content, base_hash }) => {
          const res = await fetch('/api/kiln/file', {
            method: 'PUT',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ path: file, content, base_hash }),
          });
          return { status: res.status, body: await res.json().catch(() => null) };
        },
        { file: notePath, content: body, base_hash: base },
      );

    // A first write with no base: the blind path every existing caller uses.
    const first = await put('the original\n');
    expect(first.status).toBe(200);
    const base = first.body.content_hash as string;
    expect(base).toMatch(/^[0-9a-f]{64}$/);

    // Another writer — the agent, the TUI, a second browser — gets there first.
    writeFileSync(notePath, 'ANOTHER WRITER GOT HERE\n');

    const refused = await put('what the first caller had\n', base);
    expect(refused.status).toBe(409);
    expect(refused.body.ok).toBe(false);
    expect(refused.body.current_hash).toMatch(/^[0-9a-f]{64}$/);
    expect(refused.body.current_hash).not.toBe(base);
    // The other writer's work survives, byte for byte.
    expect(readFileSync(notePath, 'utf-8')).toBe('ANOTHER WRITER GOT HERE\n');

    // The hash the refusal handed back is the one that works.
    const retried = await put('now we agree\n', refused.body.current_hash as string);
    expect(retried.status).toBe(200);
    expect(readFileSync(notePath, 'utf-8')).toBe('now we agree\n');
  });

  /**
   * WS-320: the first caller of the anchored-edit primitive.
   *
   * Ticking a box in the reading view must change ONE line. A whole-file save
   * would carry the reader's entire copy back, so an agent's edit to another
   * paragraph of the same note — made between the read and the tap — would be
   * silently erased. That is the case section 13 was written for.
   */
  test('WS-320: ticking a task changes its line and leaves a concurrent edit alone', async ({
    page,
  }) => {
    const baseURL = state.baseURL!;
    const kilnDir = state.kilnDir!;
    const notePath = path.join(kilnDir, 'Tasks.md');
    const before = '# Tasks\n\n- [ ] ship it\n- [ ] ping\n- [ ] ping\n\nnotes below\n';
    writeFileSync(notePath, before);

    await page.goto(baseURL);
    const tick = (edits: unknown[]) =>
      page.evaluate(
        async ({ file, body }) => {
          const res = await fetch('/api/kiln/file', {
            method: 'PATCH',
            headers: { 'Content-Type': 'application/json' },
            body: JSON.stringify({ path: file, edits: body }),
          });
          return { status: res.status, body: await res.json() };
        },
        { file: notePath, body: edits },
      );

    // Another writer changes a DIFFERENT paragraph after the reader loaded it.
    writeFileSync(notePath, before.replace('notes below', 'AN AGENT WROTE HERE'));

    const ticked = await tick([{ expect: '- [ ] ship it', replace: '- [x] ship it' }]);
    expect(ticked.status).toBe(200);
    expect(readFileSync(notePath, 'utf-8')).toBe(
      '# Tasks\n\n- [x] ship it\n- [ ] ping\n- [ ] ping\n\nAN AGENT WROTE HERE\n',
    );

    // Two identical lines: without an occurrence the daemon refuses as
    // ambiguous, which is why `taskEditForLine` counts identical lines.
    const ambiguous = await tick([{ expect: '- [ ] ping', replace: '- [x] ping' }]);
    expect(ambiguous.status).toBe(409);
    expect(ambiguous.body.failed[0].reason).toBe('ambiguous');

    const second = await tick([{ expect: '- [ ] ping', replace: '- [x] ping', occurrence: 1 }]);
    expect(second.status).toBe(200);
    expect(readFileSync(notePath, 'utf-8')).toBe(
      '# Tasks\n\n- [x] ship it\n- [ ] ping\n- [x] ping\n\nAN AGENT WROTE HERE\n',
    );
  });

  /**
   * WS-323: an open note hears the kiln watcher.
   *
   * A second writer — the agent, the terminal, another browser — lands a note
   * this editor holds open with unsent edits. Two things must hold at once:
   * the user's bytes are never replaced, because they exist nowhere else, and
   * the user is never left editing a text the note no longer holds. The banner
   * is how both hold: it says the disk moved and names the two ways out.
   *
   * Only a real daemon proves it. The watcher, its debounce and the SSE leg
   * are three processes' worth of plumbing that no mocked subscription runs.
   */
  test('WS-323: an open note shows the banner when a second writer PUTs it', async ({ page }) => {
    const baseURL = state.baseURL!;
    const kilnDir = state.kilnDir!;
    const notePath = path.join(kilnDir, 'Watched.md');
    writeFileSync(notePath, '# Watched\n\nthe first text\n');

    await page.goto(baseURL);
    await appReady(page);

    // The product's own open-file door — a source import would not reach the
    // bundled live app.
    await page.evaluate((file) => {
      window.dispatchEvent(
        new CustomEvent('crucible:open-file', { detail: { path: file, name: 'Watched.md' } }),
      );
    }, notePath);
    await expect(page.locator('.cm-editor')).toBeVisible({ timeout: 10_000 });
    await expect(page.locator('.cm-content')).toContainText('the first text');

    // Type, so the buffer holds bytes that are nowhere else. vimMode is on by
    // default, so `A` — append at end of line — is what makes the keystrokes
    // that follow land as literal text (the hero spec does the same dance).
    await page.locator('.cm-content').first().click();
    await page.keyboard.press('Escape');
    await page.keyboard.press('A');
    await page.keyboard.type(' and my unsent edit');
    await page.keyboard.press('Escape');
    await expect(page.locator('.cm-content')).toContainText('and my unsent edit');

    // The second writer: a blind PUT, the shape every other caller makes.
    const wrote = await page.evaluate(async (file) => {
      const res = await fetch('/api/kiln/file', {
        method: 'PUT',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ path: file, content: '# Watched\n\nANOTHER WRITER GOT HERE\n' }),
      });
      return res.status;
    }, notePath);
    expect(wrote).toBe(200);

    // The daemon's watcher debounces 500 ms; the SSE leg carries it from there.
    await expect(page.getByTestId('disk-changed-banner')).toBeVisible({ timeout: 5_000 });
    await expect(page.getByTestId('disk-changed-reload')).toBeVisible();
    await expect(page.getByTestId('disk-changed-merge')).toBeVisible();

    // Nothing of the user's was taken, and the other writer's text stands.
    await expect(page.locator('.cm-content')).toContainText('and my unsent edit');
    expect(readFileSync(notePath, 'utf-8')).toBe('# Watched\n\nANOTHER WRITER GOT HERE\n');
  });
});
