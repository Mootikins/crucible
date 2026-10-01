import { describe, it, expect, vi } from 'vitest';
import { makeMarkdownClickHandler } from '../markdown-click';
import { openFileInEditor } from '../file-actions';
import { openNoteInEditor } from '../note-actions';
import { copyText } from '../clipboard';
import { notificationActions } from '@/stores/notificationStore';
vi.mock('../clipboard', () => ({ copyText: vi.fn().mockResolvedValue(undefined) }));
vi.mock('@/stores/notificationStore', () => ({ notificationActions: { addNotification: vi.fn() } }));
vi.mock('../file-actions', () => ({
  openFileInEditor: vi.fn(),
  fileOpenOptionsForEvent: (event: MouseEvent) => ({ where: event.shiftKey ? 'split' : 'here' }),
}));
vi.mock('../note-actions', () => ({ openNoteInEditor: vi.fn(), kilnForElement: () => '/notes' }));
describe('markdown file links', () => {
  it('opens an absolute encoded path through the file owner, preserving modifiers', () => {
    const a = document.createElement('a');
    a.href = '/home/user/project/My%20File.rs';
    a.addEventListener('click', makeMarkdownClickHandler());
    a.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true, shiftKey: true }));
    expect(openFileInEditor).toHaveBeenCalledWith('/home/user/project/My File.rs', undefined, { where: 'split' });
    expect(openNoteInEditor).not.toHaveBeenCalled();
  });
  it('opens a line-qualified file link without treating the line as part of its path', () => {
    const a = document.createElement('a');
    a.setAttribute('href', '/repo/mode.rs:216');
    a.addEventListener('click', makeMarkdownClickHandler());
    a.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    expect(openFileInEditor).toHaveBeenCalledWith('/repo/mode.rs', undefined, { line: 216 });
  });
  it('keeps relative note links in the kiln', () => {
    const a = document.createElement('a');
    a.setAttribute('href', 'Guides/Start.md');
    a.addEventListener('click', makeMarkdownClickHandler());
    a.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
    expect(openNoteInEditor).toHaveBeenCalledWith('Guides/Start', '/notes');
  });
});

it('absolute links keep the originating editor tab even when another pane was focused', () => {
  const editor = document.createElement('div');
  editor.dataset.fileTabId = 'source-note';
  editor.innerHTML = '<a href="/repo/target.rs:12">Target</a>';
  editor.addEventListener('click', makeMarkdownClickHandler());
  editor.querySelector('a')!.dispatchEvent(new MouseEvent('click', { bubbles: true, cancelable: true }));
  expect(openFileInEditor).toHaveBeenLastCalledWith('/repo/target.rs', undefined, { where: 'here', tabId: 'source-note', line: 12 });
});

it('code Copy waits for clipboard success before reporting success', async () => {
  let finish!: () => void;
  vi.mocked(copyText).mockImplementationOnce(() => new Promise<void>(resolve => { finish = resolve; }));
  const block = document.createElement('div');
  block.className = 'md-codeblock';
  block.innerHTML = '<button data-copy>Copy</button><pre>sample code</pre>';
  block.addEventListener('click', makeMarkdownClickHandler());
  const button = block.querySelector('button')!;
  button.click();
  expect(copyText).toHaveBeenCalledWith('sample code');
  expect(button.textContent).toBe('Copy');
  finish();
  await vi.waitFor(() => expect(button.textContent).toBe('Copied'));
});

it('code Copy reports failures without claiming the code was copied', async () => {
  vi.mocked(copyText).mockRejectedValueOnce(new Error('blocked'));
  const block = document.createElement('div');
  block.className = 'md-codeblock';
  block.innerHTML = '<button data-copy>Copy</button><pre>sample code</pre>';
  block.addEventListener('click', makeMarkdownClickHandler());
  block.querySelector('button')!.click();
  await vi.waitFor(() => expect(notificationActions.addNotification).toHaveBeenCalledWith('error', expect.stringContaining('Copy was blocked')));
  expect(block.querySelector('button')!.textContent).toBe('Copy');
});
