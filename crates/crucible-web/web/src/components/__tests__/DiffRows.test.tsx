import { cleanup, render } from '@solidjs/testing-library';
import { afterEach, expect, it } from 'vitest';
import { analyzeDiff } from '@/lib/diff-stats';
import { DiffRows } from '@/components/DiffRows';

afterEach(cleanup);

it('word emphasis preserves both sides of a multiline replacement, including indentation', () => {
  const before = 'fn serve() {\n    run();\n}';
  const after = 'fn serve() {\n    log("<ready>");\n    run().await;\n}';
  const rows = analyzeDiff(before, after).lines;
  const { container } = render(() => <DiffRows rows={rows} emphasis />);
  const texts = [...container.querySelectorAll('.c')].map((el) => el.textContent);
  expect(texts).toEqual(rows.map((row) => row.content));
  expect(container.querySelector('ready')).toBeNull();
  expect(container.querySelectorAll('.add .diff-word').length).toBeGreaterThan(0);
  expect(container.querySelectorAll('.context .diff-word')).toHaveLength(0);
});

it('a deletion with no replacement keeps all of its text emphasized', () => {
  const rows = analyzeDiff('obsolete();\nremove();', '').lines;
  const { container } = render(() => <DiffRows rows={rows} emphasis />);
  expect([...container.querySelectorAll('.diff-word')].map((el) => el.textContent)).toEqual([
    'obsolete();',
    'remove();',
  ]);
});
