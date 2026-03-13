import {
  Component,
  Show,
  createEffect,
  onMount,
  onCleanup,
} from 'solid-js';
import { EditorView, keymap, lineNumbers, highlightActiveLine, highlightSpecialChars, drawSelection } from '@codemirror/view';
import { EditorState, StateEffect, Extension } from '@codemirror/state';
import { defaultKeymap, history, historyKeymap } from '@codemirror/commands';
import { oneDark } from '@codemirror/theme-one-dark';
import { markdown } from '@codemirror/lang-markdown';
import { useEditorSafe } from '@/contexts/EditorContext';
import { findTabByFilePath } from '@/lib/file-actions';
import { windowActions } from '@/stores/windowStore';

type LanguageSupport = ReturnType<typeof markdown>;

const getLanguageExtension = (path: string): LanguageSupport | null => {
  const ext = path.split('.').pop()?.toLowerCase() ?? '';
  return ext === 'md' ? markdown() : null;
};

// Inlined CodeMirror editor component (mirrors EditorPanel's CodeMirrorEditor)
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
          effects: StateEffect.reconfigure.of(createExtensions()),
        });
      }
    }
  });

  onCleanup(() => {
    view?.destroy();
  });

  return <div ref={containerRef} class="h-full w-full" />;
};

interface FileViewerPanelProps {
  filePath?: string;
}

const FileViewerPanel: Component<FileViewerPanelProps> = (props) => {
  const { openFile, closeFile, openFiles, isLoading, error, updateFileContent } = useEditorSafe();

  const fileData = () => openFiles().find(f => f.path === props.filePath) ?? null;

  createEffect(() => {
    if (props.filePath) {
      openFile(props.filePath);
    }
  });

  onCleanup(() => {
    if (props.filePath) {
      closeFile(props.filePath);
    }
  });

  // Sync EditorContext dirty state → windowStore tab isModified
  createEffect(() => {
    const file = openFiles().find(f => f.path === props.filePath);
    if (!props.filePath) return;
    const tabInfo = findTabByFilePath(props.filePath);
    if (tabInfo) {
      windowActions.updateTab(tabInfo.groupId, tabInfo.tab.id, {
        isModified: file?.dirty ?? false,
      });
    }
  });

  // No file path provided — nothing to render
  if (!props.filePath) {
    return (
      <div class="h-full bg-neutral-900 p-4 flex items-center justify-center text-neutral-400 text-sm">
        No file selected
      </div>
    );
  }

  return (
    <div class="h-full flex flex-col bg-neutral-900 text-neutral-100 overflow-hidden relative">
      {/* Loading overlay */}
      <Show when={isLoading()}>
        <div class="absolute inset-0 flex items-center justify-center bg-neutral-900/80 z-10">
          <div class="flex items-center gap-3">
            <div class="w-5 h-5 border-2 border-neutral-600 border-t-neutral-300 rounded-full animate-spin" />
            <span class="text-neutral-400 text-sm">Loading file...</span>
          </div>
        </div>
      </Show>

      {/* Error bar */}
      <Show when={error()}>
        <div class="mx-4 mt-2 px-3 py-2 text-sm text-red-400 bg-red-900/20 rounded border border-red-900/30 flex items-center gap-2">
          <svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 20 20" fill="currentColor" class="w-4 h-4 shrink-0">
            <path fill-rule="evenodd" d="M18 10a8 8 0 11-16 0 8 8 0 0116 0zm-8-5a.75.75 0 01.75.75v4.5a.75.75 0 01-1.5 0v-4.5A.75.75 0 0110 5zm0 10a1 1 0 100-2 1 1 0 000 2z" clip-rule="evenodd" />
          </svg>
          <span>{error()}</span>
        </div>
      </Show>

      {/* Editor area */}
      <div class="flex-1 overflow-hidden">
        <Show
          when={fileData()}
          fallback={
            <Show when={!isLoading()}>
              <div class="h-full flex items-center justify-center text-neutral-500">
                <div class="text-center">
                  <div class="text-4xl mb-4">📄</div>
                  <div class="text-sm">Loading file...</div>
                </div>
              </div>
            </Show>
          }
        >
          {(file) => (
            <CodeMirrorEditor
              content={file().content}
              path={file().path}
              onChange={(content) => updateFileContent(file().path, content)}
            />
          )}
        </Show>
      </div>
    </div>
  );
};

export default FileViewerPanel;
