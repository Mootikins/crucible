import { Component, createEffect, onCleanup } from 'solid-js';
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightSpecialChars,
  drawSelection,
} from '@codemirror/view';
import { EditorState, StateEffect, Extension, Annotation } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { oneDark } from '@codemirror/theme-one-dark';
import { markdown } from '@codemirror/lang-markdown';
import { javascript } from '@codemirror/lang-javascript';
import { rust } from '@codemirror/lang-rust';

type LanguageSupport = ReturnType<typeof markdown>;

export const getLanguageExtension = (path: string): LanguageSupport | null => {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  switch (ext) {
    case 'md':
    case 'markdown':
      return markdown();
    case 'js':
    case 'mjs':
    case 'cjs':
      return javascript();
    case 'jsx':
      return javascript({ jsx: true });
    case 'ts':
      return javascript({ typescript: true });
    case 'tsx':
      return javascript({ typescript: true, jsx: true });
    case 'rs':
      return rust();
    default:
      return null;
  }
};

// Marks dispatches that mirror external state (file switch, reload) into the
// reused view, so the update listener can tell them apart from user edits —
// otherwise switching the active file marks the incoming file dirty (bug 5).
export const contentSync = Annotation.define<boolean>();

export const CodeMirrorEditor: Component<{
  content: string;
  path: string;
  onChange: (content: string) => void;
  onSave?: () => void;
}> = (props) => {
  let view: EditorView | undefined;

  const createExtensions = (): Extension[] => {
    const extensions: Extension[] = [
      lineNumbers(),
      highlightActiveLine(),
      highlightSpecialChars(),
      drawSelection(),
      history(),
      // Cmd/Ctrl-S saves through EditorContext.saveFile. Placed before
      // defaultKeymap so it wins, and preventDefault stops the browser
      // "save page" dialog.
      keymap.of([
        {
          key: 'Mod-s',
          preventDefault: true,
          run: () => {
            props.onSave?.();
            return true;
          },
        },
      ]),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      oneDark,
      EditorView.updateListener.of((update) => {
        if (
          update.docChanged &&
          !update.transactions.some((tr) => tr.annotation(contentSync))
        ) {
          props.onChange(update.state.doc.toString());
        }
      }),
      EditorView.theme({
        '&': { height: '100%' },
        '.cm-scroller': { overflow: 'auto' },
      }),
    ];

    const langExt = getLanguageExtension(props.path);
    if (langExt) {
      extensions.push(langExt);
    }

    return extensions;
  };

  // The view is created from the ref callback, not onMount: vitest's solid
  // pipeline fires onMount BEFORE ref callbacks (production compile is the
  // reverse), so an onMount-based init silently never mounts under jsdom
  // tests. The ref callback is the only hook guaranteed to have the element
  // in both pipelines.
  const initEditor = (el: HTMLDivElement) => {
    if (view) return;

    view = new EditorView({
      state: EditorState.create({
        doc: props.content,
        extensions: createExtensions(),
      }),
      parent: el,
    });
  };

  createEffect(() => {
    const newContent = props.content;
    if (view && view.state.doc.toString() !== newContent) {
      view.dispatch({
        changes: {
          from: 0,
          to: view.state.doc.length,
          insert: newContent,
        },
        annotations: contentSync.of(true),
      });
    }
  });

  createEffect(() => {
    // Reconfigure unconditionally so switching to a language-less file drops
    // the previous file's highlighting instead of keeping it.
    props.path;
    if (view) {
      view.dispatch({
        effects: StateEffect.reconfigure.of(createExtensions()),
      });
    }
  });

  onCleanup(() => {
    view?.destroy();
  });

  return <div ref={initEditor} class="h-full w-full" />;
};
