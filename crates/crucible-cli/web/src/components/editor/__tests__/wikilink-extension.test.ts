import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { EditorView } from '@codemirror/view';
import { EditorState } from '@codemirror/state';
import {
  wikilinkNavigation,
  wikilinkTargetAt,
  followWikilinkAtCursor,
} from '../wikilink-extension';

function makeView(doc: string, onFollow: (t: string) => void = () => {}): EditorView {
  const parent = document.createElement('div');
  document.body.appendChild(parent);
  return new EditorView({
    state: EditorState.create({ doc, extensions: [wikilinkNavigation(onFollow)] }),
    parent,
  });
}

let views: EditorView[] = [];

function track(view: EditorView): EditorView {
  views.push(view);
  return view;
}

beforeEach(() => {
  views = [];
});

afterEach(() => {
  views.forEach((v) => v.destroy());
  document.body.innerHTML = '';
});

describe('wikilink decorations', () => {
  it('marks [[wikilinks]] with .cm-wikilink and a data-note attribute', () => {
    const view = track(makeView('See [[My Note]] for details.'));
    const link = view.dom.querySelector('.cm-wikilink');
    expect(link).not.toBeNull();
    expect(link!.getAttribute('data-note')).toBe('My Note');
    expect(link!.textContent).toBe('[[My Note]]');
  });

  it('resolves aliased and fragmented links to the bare target', () => {
    const view = track(makeView('[[Target|shown]] and [[Other#Heading]]'));
    const links = view.dom.querySelectorAll('.cm-wikilink');
    expect(links).toHaveLength(2);
    expect(links[0].getAttribute('data-note')).toBe('Target');
    expect(links[1].getAttribute('data-note')).toBe('Other');
  });

  it('decorates links added by later edits', () => {
    const view = track(makeView('plain text'));
    expect(view.dom.querySelector('.cm-wikilink')).toBeNull();
    view.dispatch({ changes: { from: 0, insert: '[[New Note]] ' } });
    expect(view.dom.querySelector('.cm-wikilink')?.getAttribute('data-note')).toBe('New Note');
  });
});

describe('wikilinkTargetAt', () => {
  it('finds the target when the position is inside a link', () => {
    const view = track(makeView('before [[My Note]] after'));
    // Position inside "[[My Note]]" (starts at 7).
    expect(wikilinkTargetAt(view.state, 10)).toBe('My Note');
  });

  it('returns null outside links', () => {
    const view = track(makeView('before [[My Note]] after'));
    expect(wikilinkTargetAt(view.state, 2)).toBeNull();
    expect(wikilinkTargetAt(view.state, view.state.doc.length)).toBeNull();
  });
});

describe('follow gestures', () => {
  it('Mod-Enter command follows the link under the cursor', () => {
    const onFollow = vi.fn();
    const view = track(makeView('go to [[My Note]] now', onFollow));
    view.dispatch({ selection: { anchor: 10 } });

    const handled = followWikilinkAtCursor(onFollow)(view);
    expect(handled).toBe(true);
    expect(onFollow).toHaveBeenCalledWith('My Note');
  });

  it('Mod-Enter command declines when the cursor is not in a link', () => {
    const onFollow = vi.fn();
    const view = track(makeView('go to [[My Note]] now', onFollow));
    view.dispatch({ selection: { anchor: 2 } });

    expect(followWikilinkAtCursor(onFollow)(view)).toBe(false);
    expect(onFollow).not.toHaveBeenCalled();
  });

  it('Ctrl+Click on a decorated link follows it', () => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    const link = view.dom.querySelector('.cm-wikilink')!;

    link.dispatchEvent(new MouseEvent('mousedown', { bubbles: true, ctrlKey: true }));
    expect(onFollow).toHaveBeenCalledWith('My Note');
  });

  it('plain click does not follow (text editing stays untouched)', () => {
    const onFollow = vi.fn();
    const view = track(makeView('see [[My Note]]', onFollow));
    const link = view.dom.querySelector('.cm-wikilink')!;

    link.dispatchEvent(new MouseEvent('mousedown', { bubbles: true }));
    expect(onFollow).not.toHaveBeenCalled();
  });
});
