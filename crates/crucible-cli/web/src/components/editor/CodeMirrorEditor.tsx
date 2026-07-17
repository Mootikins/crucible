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
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { yamlFrontmatter } from '@codemirror/lang-yaml';
import { javascript } from '@codemirror/lang-javascript';
import { rust } from '@codemirror/lang-rust';
import { vim } from '@replit/codemirror-vim';
import { wikilinkNavigation } from './wikilink-extension';
import { crucibleEditorChrome } from './editor-theme';
import { livePreview } from './live-preview';

type LanguageSupport = ReturnType<typeof markdown>;

export const getLanguageExtension = (path: string): LanguageSupport | null => {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  switch (ext) {
    case 'md':
    case 'markdown':
      // GFM base (strikethrough, tables, task lists) — the commonmark
      // default has no Strikethrough node for live preview to style.
      // yamlFrontmatter parses a leading `---` block as real YAML instead
      // of letting it misparse as headings/hr.
      return yamlFrontmatter({ content: markdown({ base: markdownLanguage }) });
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
  /** Follow a [[wikilink]] (Ctrl/Cmd+Click or Mod-Enter); markdown files only. */
  onFollowLink?: (target: string) => void;
  /** Modal vim editing (@replit/codemirror-vim). */
  vimMode?: boolean;
  /** Obsidian-style live preview (markdown files only). */
  livePreview?: boolean;
  /** Readable line length in px for live preview (0/undefined = full). */
  lineWidth?: number;
  /** Switch to the rendered preview (Mod-Shift-E). */
  onTogglePreview?: () => void;
}> = (props) => {
  let view: EditorView | undefined;

  const createExtensions = (): Extension[] => {
    const ext0 = props.path.split('.').pop()?.toLowerCase() ?? '';
    const liveMode = !!props.livePreview && (ext0 === 'md' || ext0 === 'markdown');
    const extensions: Extension[] = [
      // vim() must precede other keymaps so modal keys win while active.
      ...(props.vimMode ? [vim()] : []),
      // Live preview reads as prose, like the rendered view — no gutter.
      ...(liveMode ? [] : [lineNumbers()]),
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
        // Mod-Enter also saves. The wikilink follow keymap sits at Prec.high
        // and claims Mod-Enter when the cursor is ON a link; elsewhere it
        // returns false and this binding wins (shadowing insertBlankLine —
        // deliberate: save is the more useful default here).
        {
          key: 'Mod-Enter',
          preventDefault: true,
          run: () => {
            if (!props.onSave) return false;
            props.onSave();
            return true;
          },
        },
        // Mod-Shift-E, not Mod-E: vim owns Ctrl-E (scroll line) when active.
        {
          key: 'Mod-Shift-e',
          preventDefault: true,
          run: () => {
            if (!props.onTogglePreview) return false;
            props.onTogglePreview();
            return true;
          },
        },
      ]),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      // Chrome BEFORE oneDark: earlier extensions take precedence in CM6,
      // so the shell background/gutter overrides win over oneDark's.
      crucibleEditorChrome,
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

    const ext = props.path.split('.').pop()?.toLowerCase() ?? '';
    if (props.onFollowLink && (ext === 'md' || ext === 'markdown')) {
      extensions.push(wikilinkNavigation((target) => props.onFollowLink?.(target)));
    }
    if (liveMode) {
      extensions.push(livePreview({ maxLineWidth: props.lineWidth }));
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
    // the previous file's highlighting instead of keeping it. Also tracks
    // vimMode and livePreview so mode toggles apply to open editors.
    props.path;
    props.vimMode;
    props.livePreview;
    props.lineWidth;
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
