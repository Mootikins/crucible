import { Component, createEffect, onCleanup } from 'solid-js';
import {
  EditorView,
  keymap,
  lineNumbers,
  highlightActiveLine,
  highlightSpecialChars,
  drawSelection,
  dropCursor,
} from '@codemirror/view';
import { EditorState, StateEffect, Extension, Annotation, Compartment } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { LanguageDescription } from '@codemirror/language';
import { oneDark } from '@codemirror/theme-one-dark';
import { markdown, markdownLanguage } from '@codemirror/lang-markdown';
import { languages as codeLanguages } from '@codemirror/language-data';
import { yamlFrontmatter } from '@codemirror/lang-yaml';
import { javascript } from '@codemirror/lang-javascript';
import { rust } from '@codemirror/lang-rust';
import { vim } from '@replit/codemirror-vim';
import { wikilinkNavigation } from './wikilink-extension';
import { attachFileDropTarget, insertTextFor } from '@/lib/file-dnd';
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
      // of letting it misparse as headings/hr. codeLanguages nests real
      // grammars inside ```lang fences so fenced code highlights (grammars
      // lazy-load per language on first use).
      return yamlFrontmatter({
        content: markdown({ base: markdownLanguage, codeLanguages }),
      });
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
  /** Hand the live EditorView to the parent (context-menu clipboard ops). */
  apiRef?: (view: EditorView) => void;
}> = (props) => {
  let view: EditorView | undefined;
  // Holds the language extension so a lazily-loaded grammar (or a file switch)
  // can swap it in without rebuilding the editor.
  const langCompartment = new Compartment();

  // Eager grammars (getLanguageExtension) highlight instantly; anything else
  // (TOML, JSON, Python, Go, shell, CSS, …) is resolved by filename against
  // @codemirror/language-data and its grammar lazy-loaded, then swapped into
  // the compartment. Skips markdown, which is always eager (live preview).
  const applyLanguage = async (v: EditorView, path: string): Promise<void> => {
    const eager = getLanguageExtension(path);
    if (eager) {
      v.dispatch({ effects: langCompartment.reconfigure(eager) });
      return;
    }
    const filename = path.split('/').pop() ?? path;
    const desc = LanguageDescription.matchFilename(codeLanguages, filename);
    if (!desc) {
      v.dispatch({ effects: langCompartment.reconfigure([]) });
      return;
    }
    try {
      const support = await desc.load();
      if (view === v) v.dispatch({ effects: langCompartment.reconfigure(support) });
    } catch {
      /* grammar failed to load — leave plain text */
    }
  };

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
      // Insertion-point feedback while dragging a file (or text) over the doc.
      dropCursor(),
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

    // Eager grammar (or empty) up front; applyLanguage() lazy-loads the rest
    // into this compartment once the view exists.
    extensions.push(langCompartment.of(getLanguageExtension(props.path) ?? []));

    const ext = props.path.split('.').pop()?.toLowerCase() ?? '';
    if (props.onFollowLink && (ext === 'md' || ext === 'markdown')) {
      extensions.push(wikilinkNavigation((target) => props.onFollowLink?.(target)));
    }
    if (liveMode) {
      extensions.push(
        livePreview({
          maxLineWidth: props.lineWidth,
          baseDir: props.path.replace(/\/[^/]*$/, ''),
        }),
      );
    }

    return extensions;
  };

  // The view is created from the ref callback, not onMount: vitest's solid
  // pipeline fires onMount BEFORE ref callbacks (production compile is the
  // reverse), so an onMount-based init silently never mounts under jsdom
  // tests. The ref callback is the only hook guaranteed to have the element
  // in both pipelines.
  let detachFileDrop: (() => void) | undefined;

  const initEditor = (el: HTMLDivElement) => {
    if (view) return;

    view = new EditorView({
      state: EditorState.create({
        doc: props.content,
        extensions: createExtensions(),
      }),
      parent: el,
    });
    props.apiRef?.(view);
    // Lazy-load a language-data grammar for non-eager file types (TOML, etc.).
    void applyLanguage(view, props.path);

    // File-tree drops (pragmatic-drag-and-drop 'editor' zone — the innermost
    // file target, so it wins over the pane's open-in-pane zone). Kiln notes
    // dropped into markdown insert a [[wikilink]] at the pointer; anything
    // else inserts the root-relative path.
    const v = view;
    detachFileDrop = attachFileDropTarget(v.dom, {
      zone: 'editor',
      canDrop: (source) => !source.isDir,
      onDrop: (source, input) => {
        const pos =
          v.posAtCoords({ x: input.clientX, y: input.clientY }) ??
          v.state.selection.main.head;
        const text = insertTextFor(source, props.path);
        v.dispatch({
          changes: { from: pos, insert: text },
          selection: { anchor: pos + text.length },
        });
        v.focus();
      },
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
      // createExtensions seeds the compartment with the eager grammar (or
      // empty); re-run the lazy resolver so a switched-to TOML/JSON/etc. file
      // gets its grammar too.
      void applyLanguage(view, props.path);
    }
  });

  onCleanup(() => {
    detachFileDrop?.();
    view?.destroy();
  });

  return <div ref={initEditor} class="h-full w-full" />;
};
