import {
  Component,
  For,
  Show,
  createSignal,
  createEffect,
  onMount,
  onCleanup,
} from 'solid-js';
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightSpecialChars, drawSelection } from '@codemirror/view';
import { EditorState, Extension } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { oneDark } from '@codemirror/theme-one-dark';
import { markdown } from '@codemirror/lang-markdown';
import { javascript } from '@codemirror/lang-javascript';
import { rust } from '@codemirror/lang-rust';
import { useEditor } from '@/contexts/EditorContext';

type LanguageSupport = ReturnType<typeof markdown>;

const getLanguageExtension = (path: string): LanguageSupport | null => {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  
  const langMap: Record<string, () => LanguageSupport> = {
    md: () => markdown(),
    ts: () => javascript({ typescript: true }),
    tsx: () => javascript({ typescript: true, jsx: true }),
    js: () => javascript(),
    jsx: () => javascript({ jsx: true }),
    rs: () => rust(),
  };
  
  const factory = langMap[ext];
  return factory ? factory() : null;
};

const getFilename = (path: string): string => {
  return path.split('/').pop() ?? path;
};

const Tab: Component<{
  path: string;
  active: boolean;
  dirty: boolean;
  onSelect: () => void;
  onClose: () => void;
}> = (props) => {
  const handleClose = (e: MouseEvent) => {
    e.stopPropagation();
    props.onClose();
  };

  return (
    <button
      class="flex items-center gap-2 px-3 py-1.5 text-sm border-b-2 transition-colors whitespace-nowrap"
      classList={{
        'border-blue-500 text-neutral-100 bg-neutral-800': props.active,
        'border-transparent text-neutral-400 hover:text-neutral-200 hover:bg-neutral-800/50': !props.active,
      }}
      onClick={props.onSelect}
    >
      <span class="truncate max-w-[150px]">
        {props.dirty && <span class="text-blue-400 mr-1">●</span>}
        {getFilename(props.path)}
      </span>
      <span
        class="text-neutral-500 hover:text-neutral-200 hover:bg-neutral-700 rounded px-1"
        onClick={handleClose}
      >
        ×
      </span>
    </button>
  );
};

const CodeMirrorEditor: Component<{
  content: string;
  path: string;
  onChange: (content: string) => void;
}> = (props) => {
  let containerRef: HTMLDivElement | undefined;
  let view: EditorView | undefined;

  const createExtensions = (): Extension[] => {
    const extensions: Extension[] = [
      lineNumbers(),
      highlightActiveLine(),
      highlightSpecialChars(),
      drawSelection(),
      history(),
      keymap.of([...defaultKeymap, ...historyKeymap]),
      oneDark,
      EditorView.updateListener.of((update) => {
        if (update.docChanged) {
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

  onMount(() => {
    if (!containerRef) return;

    const state = EditorState.create({
      doc: props.content,
      extensions: createExtensions(),
    });

    view = new EditorView({
      state,
      parent: containerRef,
    });
  });

  createEffect(() => {
    const newContent = props.content;
    if (view && view.state.doc.toString() !== newContent) {
      view.dispatch({
        changes: {
          from: 0,
          to: view.state.doc.length,
          insert: newContent,
        },
      });
    }
  });

  createEffect(() => {
    const currentPath = props.path;
    if (view) {
      const langExt = getLanguageExtension(currentPath);
      if (langExt) {
        view.dispatch({
          effects: EditorView.reconfigure.of(createExtensions()),
        });
      }
    }
  });

  onCleanup(() => {
    view?.destroy();
  });

  return <div ref={containerRef} class="h-full w-full" />;
};

export const EditorPanel: Component = () => {
  const { openFiles, activeFile, setActiveFile, closeFile, updateFileContent, isLoading } = useEditor();

  const activeFileData = () => {
    const path = activeFile();
    if (!path) return null;
    return openFiles().find((f) => f.path === path) ?? null;
  };

  return (
    <div class="h-full flex flex-col bg-neutral-900 text-neutral-100 overflow-hidden">
      <Show
        when={openFiles().length > 0}
        fallback={
          <div class="flex-1 flex items-center justify-center text-neutral-500">
            <div class="text-center">
              <div class="text-4xl mb-4">📄</div>
              <div class="text-sm">No files open</div>
              <div class="text-xs text-neutral-600 mt-1">Click a file in the sidebar to open it</div>
            </div>
          </div>
        }
      >
        <div class="flex border-b border-neutral-800 bg-neutral-900 overflow-x-auto shrink-0">
          <For each={openFiles()}>
            {(file) => (
              <Tab
                path={file.path}
                active={file.path === activeFile()}
                dirty={file.dirty}
                onSelect={() => setActiveFile(file.path)}
                onClose={() => closeFile(file.path)}
              />
            )}
          </For>
        </div>

        <Show when={isLoading()}>
          <div class="absolute inset-0 flex items-center justify-center bg-neutral-900/80 z-10">
            <div class="text-neutral-400">Loading...</div>
          </div>
        </Show>

        <div class="flex-1 overflow-hidden relative">
          <Show when={activeFileData()}>
            {(file) => (
              <CodeMirrorEditor
                content={file().content}
                path={file().path}
                onChange={(content) => updateFileContent(file().path, content)}
              />
            )}
          </Show>
        </div>
      </Show>
    </div>
  );
};
