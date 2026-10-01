import { test, expect } from '@playwright/test';
import { mkdirSync, writeFileSync, readFileSync } from 'node:fs';
import { setupBasicMocks } from './helpers/mock-api';
import { appReady, openSession } from './helpers/nav';
import { MOCK_SESSION } from './helpers/fixtures';
import { compareImages } from './helpers/image-comparison';
import { userTurn, segment } from '../src/test-utils/transcript';

// An independent, pre-migration worktree is required because the design fixture
// is retired. Only the reference server loads the original mockup modules.
const reference = process.env.CRUCIBLE_MOCKUP_REFERENCE_URL;
const output = process.env.CRUCIBLE_PARITY_OUTPUT ?? '/tmp/crucible-visual-parity';
test.skip(!reference, 'Set CRUCIBLE_MOCKUP_REFERENCE_URL to the pre-migration mockup server.');
for (const theme of ['dark', 'light'] as const) {
  test(`independent mockup comparison ${theme}`, async ({ page, context }) => {
    mkdirSync(output, { recursive: true });
    await page.setViewportSize({ width: 1600, height: 960 });
    await page.addInitScript((theme) => {
      localStorage.setItem('crucible:theme', theme);
    }, theme);
    const mock = await context.newPage();
    await mock.setViewportSize({ width: 1600, height: 960 });
    await mock.addInitScript((theme) => localStorage.setItem('crucible-shell-mockup-tweaks-v2', JSON.stringify({ theme })), theme);
    await mock.goto(`${reference}/shell-mockup.html?review`);
    await expect(mock.locator('.mk-perm')).toBeVisible();
    await mock.evaluate(async () => {
      const { setState } = await import('/src/test-harness/shell-mockup/state.ts');
      setState('sessions', 's1', 'title', 'Test Session');
      setState('transcripts', 's1', [
        { t: 'user', text: 'Add a short section on when to turn precognition off.', time: '' },
        { t: 'text', md: 'I will add it after **Checking Current Settings**.', elapsed: '1 s' },
      ]);
      setState('perms', 's1', undefined);
    });
    await setupBasicMocks(page, { sessionHistory: {
      session_id: MOCK_SESSION.session_id, history: [], total_events: 2,
      transcript: { as_of_seq: 2, items: [userTurn('turn', 'Add a short section on when to turn precognition off.'), segment('turn', 0, 'I will add it after **Checking Current Settings**.')] },
    } });
    await page.route(/\/api\/events\?.*/, () => new Promise(() => {}));
    await page.goto('/');
    await appReady(page);
    await openSession(page, MOCK_SESSION.session_id);
    await page.evaluate(() => document.fonts.ready);
    await mock.evaluate(() => document.fonts.ready);
    await mock.screenshot({ path: `${output}/${theme}-reference-full.png` });
    await page.screenshot({ path: `${output}/${theme}-production-full.png` });
    writeFileSync(`${output}/${theme}-session-styles.json`, JSON.stringify(await Promise.all([mock, page].map(target => target.locator('[data-testid=edge-host-right]').evaluate(root => [...root.querySelectorAll('*')].filter(el => el.textContent === 'Add a short section on when to turn precognition off.' || el.getAttribute('placeholder')?.includes('message')).map(el => { const s = getComputedStyle(el); return { tag: el.tagName, cls: el.className, font: s.fontFamily, size: s.fontSize, weight: s.fontWeight, lineHeight: s.lineHeight, maxWidth: s.maxWidth, rect: el.getBoundingClientRect().toJSON() }; })))), null, 2));
    await mock.locator('[data-testid=edge-host-right]').screenshot({ path: `${output}/${theme}-reference-session.png` });
    await page.locator('[data-testid=edge-host-right]').screenshot({ path: `${output}/${theme}-production-session.png` });
    const transcriptGeometry = [];
    for (const [target, production] of [[mock, false], [page, true]] as const) {
      const bubble = (await target.locator(production ? '.user-quote' : '.mk-bubble').boundingBox())!;
      const copy = (await target.getByRole('button', { name: 'Copy response', exact: true }).boundingBox())!;
      const regenerate = (await target.getByRole('button', { name: 'Regenerate response', exact: true }).boundingBox())!;
      await target.screenshot({ path: `${output}/${theme}-${production ? 'production' : 'reference'}-turn-actions.png`, clip: { x: copy.x, y: copy.y, width: regenerate.x + regenerate.width - copy.x, height: copy.height } });
      const paragraph = target.locator('[data-testid=edge-host-right] p').filter({ hasText: 'I will add it after' });
      const text = (await paragraph.boundingBox())!;
      const clip = { x: text.x, y: bubble.y, width: text.width, height: text.y + text.height - bubble.y };
      transcriptGeometry.push({ clip, strong: await paragraph.locator('strong').evaluate(el => { const s = getComputedStyle(el); return { weight: s.fontWeight, size: s.fontSize, lineHeight: s.lineHeight }; }) });
      await target.screenshot({ path: `${output}/${theme}-${production ? 'production' : 'reference'}-transcript.png`, clip });
    }
    const actionsComparison = await compareImages(page, readFileSync(`${output}/${theme}-reference-turn-actions.png`), readFileSync(`${output}/${theme}-production-turn-actions.png`), `${output}/${theme}-turn-actions`);
    const transcriptComparison = await compareImages(page, readFileSync(`${output}/${theme}-reference-transcript.png`), readFileSync(`${output}/${theme}-production-transcript.png`), `${output}/${theme}-transcript`);
    writeFileSync(`${output}/${theme}-transcript-geometry.json`, JSON.stringify(transcriptGeometry, null, 2));
    const samples = [];
    // Matched fixture content isolates the rows' typography, syntax, geometry,
    // tints and inline emphasis. No screenshot masks or style overrides.
    for (const [target, production] of [[mock, false], [page, true]] as const) {
      await target.evaluate(async ({ production }) => {
        const [{ render }, { createComponent }, { analyzeDiff }] = await Promise.all([
          import('/node_modules/.vite/deps/solid-js_web.js'),
          import('/node_modules/.vite/deps/solid-js.js'),
          import('/src/lib/diff-stats.ts'),
        ]);
        const oldContent = 'const greeting = "Hello world";\nconst count = 2;\nconsole.log(greeting, count);\n';
        const newContent = 'const greeting = "Hello Crucible";\nconst count = 3;\nconsole.log(greeting, count);\n';
        const mod = production ? await import('/src/components/DiffViewer.tsx') : await import('/src/test-harness/shell-mockup/components/session/PermissionDiff.tsx');
        const host = document.createElement('div');
        host.id = 'parity-fixture';
        // Equal test surface dimensions; styling of the rendered components is untouched.
        Object.assign(host.style, { position: 'fixed', left: '0px', top: '0px', width: '640px', zIndex: '99999', background: 'var(--cru-color-surface-base)', boxSizing: 'border-box', padding: production ? '0 12px' : '0' });
        document.body.append(host);
        const analysis = analyzeDiff(oldContent, newContent);
        render(() => createComponent(production ? mod.DiffViewer : mod.PermissionDiff, production
          ? { oldContent, newContent, fileName: 'greeting.ts' }
          : { path: 'greeting.ts', add: analysis.additions, del: analysis.deletions, rows: analysis.lines }), host);
      }, { production });
      const rows = target.locator(production ? '#parity-fixture .diff-rows .diff-rows' : '#parity-fixture .mk-diff-rows');
      await expect(rows.locator('span[style*="color"]').first()).toBeVisible();
      await target.evaluate(() => document.fonts.ready);
      const label = production ? 'production' : 'reference';
      await target.locator('#parity-fixture').screenshot({ path: `${output}/${theme}-${label}-permission-diff.png` });
      const rowElements = rows.locator(production ? '.diff-row' : '.mk-diff-row');
      const first = (await rowElements.first().boundingBox())!, last = (await rowElements.last().boundingBox())!;
      // Compare the rows, excluding the permission viewport's bottom spacer.
      await target.screenshot({ path: `${output}/${theme}-${label}-diff-rows.png`, clip: { x: first.x, y: first.y, width: first.width, height: last.y + last.height - first.y } });
      samples.push({ label, columns: await rows.locator(production ? '.diff-row' : '.mk-diff-row').first().evaluate(el => getComputedStyle(el).gridTemplateColumns.split(' ').slice(0, 3)), geometry: await rows.boundingBox(), styles: await rows.evaluate(el => {
        const s = getComputedStyle(el); return { fonts: [...document.fonts].filter(f=>f.family.includes('Mono')).map(f=>({family:f.family,status:f.status})), rowFont: getComputedStyle(el.firstElementChild!).font, rowLetterSpacing: getComputedStyle(el.firstElementChild!).letterSpacing, font: s.fontFamily, size: s.fontSize, lineHeight: s.lineHeight, background: s.backgroundColor };
      }) });
    }
    writeFileSync(`${output}/${theme}-measurements.json`, JSON.stringify(samples, null, 2));
    for (const [target, production] of [[mock, false], [page, true]] as const) {
      await target.evaluate(async ({ production }) => {
        document.querySelector('#parity-fixture')?.remove();
        const [{ render }, { createComponent }, { analyzeDiff }] = await Promise.all([
          import('/node_modules/.vite/deps/solid-js_web.js'), import('/node_modules/.vite/deps/solid-js.js'), import('/src/lib/diff-stats.ts'),
        ]);
        const oldContent = 'const greeting = "Hello world";\nconst count = 2;\nconsole.log(greeting, count);\n';
        const newContent = 'const greeting = "Hello Crucible";\nconst count = 3;\nconsole.log(greeting, count);\n';
        const mod = production ? await import('/src/components/interactions/PermissionInteraction.tsx') : await import('/src/test-harness/shell-mockup/components/session/PermissionCard.tsx');
        const host = document.createElement('div'); host.id = 'parity-card';
        Object.assign(host.style, { position: 'fixed', left: '0px', top: '0px', width: '368px', zIndex: '99999', background: 'var(--cru-color-surface-base)' });
        document.body.append(host);
        const analysis = analyzeDiff(oldContent, newContent);
        render(() => createComponent(production ? mod.PermissionInteraction : mod.PermissionCard, production
          ? { request: { kind: 'permission', action: { type: 'write', segments: ['greeting.ts'] }, pattern: 'greeting.ts', diffs: [{ path: 'greeting.ts', old_content: oldContent, new_content: newContent }] }, onRespond: () => {} }
          : { file: 'greeting.ts', diff: { path: 'greeting.ts', add: analysis.additions, del: analysis.deletions, rows: analysis.lines }, onAnswer: () => {} }), host);
      }, { production });
      await target.evaluate(() => document.fonts.ready);
      await target.locator('#parity-card').screenshot({ path: `${output}/${theme}-${production ? 'production' : 'reference'}-permission-card.png` });
    }
    await compareImages(page, readFileSync(`${output}/${theme}-reference-session.png`), readFileSync(`${output}/${theme}-production-session.png`), `${output}/${theme}-session`);
    const rowsComparison = await compareImages(page, readFileSync(`${output}/${theme}-reference-diff-rows.png`), readFileSync(`${output}/${theme}-production-diff-rows.png`), `${output}/${theme}-rows`);
    writeFileSync(`${output}/${theme}-report.html`, `<!doctype html><meta charset="utf-8"><title>Mockup parity: ${theme}</title><style>body{background:#222;color:#eee;font:14px system-ui;padding:20px}.pair{display:flex;gap:16px;align-items:flex-start;overflow:auto}.pair img{max-width:48%}</style><h1>Independent mockup parity: ${theme}</h1><p>Reference left; production right. Identical minimal transcript and edit. Loaded Geist fonts, 1600×960. Full pages contain different fixtures and are context only. Permission decision controls are retained. Diff rows have a strict 0.5% changed-pixel gate at max-channel threshold16.</p>${['session', 'transcript', 'permission-card', 'permission-diff', 'diff-rows', 'full'].map(region => `<h2>${region}</h2><div class="pair"><img src="${theme}-reference-${region}.png"><img src="${theme}-production-${region}.png"></div>`).join('')}<h2>Row difference / overlay</h2><div class="pair"><img src="${theme}-rows-diff.png"><img src="${theme}-rows-overlay.png"></div><p>${JSON.stringify(rowsComparison)}</p>`);
    await mock.close();
    for (const sample of samples) {
      expect(sample.styles.fonts.some(font => font.status === 'loaded'), `${sample.label}: comparison requires loaded Geist Mono`).toBe(true);
      expect(sample.styles.fonts.some(font => font.status === 'error')).toBe(false);
    }
    expect(samples[1].styles).toEqual(samples[0].styles);
    expect(samples[1].columns).toEqual(samples[0].columns);
    expect(actionsComparison.changedRatio, 'Completed-turn copy and regenerate icons must match').toBeLessThan(0.005);
    expect(transcriptComparison.changedRatio, 'Matched user and assistant transcript must preserve original styling').toBeLessThan(0.005);
    expect(rowsComparison.reference).toEqual(rowsComparison.production);
    expect(rowsComparison.changedRatio, 'Matched diff rows must agree with the independent original mockup').toBeLessThan(0.005);
  });
}
