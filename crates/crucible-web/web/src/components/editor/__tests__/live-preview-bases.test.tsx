import { afterEach, describe, expect, it, vi } from 'vitest';
import { cleanup, render } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { EditorView } from '@codemirror/view';
import type { BaseViewProps } from '@/components/bases/BaseView';
import { CodeMirrorEditor } from '../CodeMirrorEditor';

/** The props of each base the editor mounted, as live objects. */
const mounted = vi.hoisted(() => [] as BaseViewProps[]);
vi.mock('@/components/bases/BaseView', () => ({
  BaseView: (props: BaseViewProps) => {
    mounted.push(props);
    return null;
  },
}));

const views: EditorView[] = [];
afterEach(() => {
  for (const view of views.splice(0)) view.destroy();
  mounted.length = 0;
  cleanup();
});

describe('bases in live preview', () => {
  it('a base embed reads the kiln the buffer has now, not the kiln at editor start', () => {
    const [kiln, setKiln] = createSignal<string | undefined>(undefined);
    const { container } = render(() => (
      <CodeMirrorEditor content={'Intro\n\n![[Tasks.base#Board]]\n\n![[Other.base]]\n'} path="/kiln/Host.md" kiln={kiln()} onChange={() => {}} livePreview />
    ));
    views.push(EditorView.findFromDOM(container.querySelector('.cm-content') as HTMLElement)!);
    // CodeMirror can draw a widget more than once; each copy must follow.
    const boards = mounted.filter(props => props.filePath === 'Tasks.base');
    const board = boards[0];
    const other = mounted.find(props => props.filePath === 'Other.base');
    expect(boards.length).toBeGreaterThan(0);
    expect(board!.kiln).toBeUndefined();

    setKiln('Work');
    for (const props of boards) expect(props.kiln).toBe('Work');
    expect(board!.filePath).toBe('Tasks.base');
    expect(board!.view).toBe('Board');
    expect(board!.host).toBe('/kiln/Host.md');
    expect(other!.view).toBeUndefined();
  });
});
