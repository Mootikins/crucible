import { Component, For, Show, createSignal, createEffect, createMemo } from 'solid-js';
import { Collapsible } from '@ark-ui/solid';
import { useProject } from '@/contexts/ProjectContext';
import { useEditor } from '@/contexts/EditorContext';
import { listFiles, listKilnNotes } from '@/lib/api';
import type { FileEntry } from '@/lib/types';

interface FileNode {
  id: string;
  name: string;
  path: string;
  is_dir: boolean;
  children?: FileNode[];
}

const FileIcon: Component<{ extension: string }> = (props) => {
  const icon = createMemo(() => {
    const ext = props.extension.toLowerCase();
    switch (ext) {
      case 'md':
        return '📝';
      case 'ts':
      case 'tsx':
        return '🔷';
      case 'js':
      case 'jsx':
        return '🟨';
      case 'rs':
        return '🦀';
      case 'json':
        return '📋';
      case 'toml':
      case 'yaml':
      case 'yml':
        return '⚙️';
      case 'css':
      case 'scss':
        return '🎨';
      case 'html':
        return '🌐';
      case 'lua':
      case 'fnl':
        return '🌙';
      default:
        return '📄';
    }
  });

  return <span class="mr-1.5 text-sm">{icon()}</span>;
};

const FolderIcon: Component<{ open?: boolean }> = (props) => (
  <span class="mr-1.5 text-sm">{props.open ? '📂' : '📁'}</span>
);

const ChevronIcon: Component<{ open?: boolean }> = (props) => (
  <svg
    xmlns="http://www.w3.org/2000/svg"
    viewBox="0 0 20 20"
    fill="currentColor"
    class="w-3.5 h-3.5 transition-transform shrink-0"
    classList={{ 'rotate-90': props.open }}
  >
    <path
      fill-rule="evenodd"
      d="M7.21 14.77a.75.75 0 01.02-1.06L11.168 10 7.23 6.29a.75.75 0 111.04-1.08l4.5 4.25a.75.75 0 010 1.08l-4.5 4.25a.75.75 0 01-1.06-.02z"
      clip-rule="evenodd"
    />
  </svg>
);

const getExtension = (filename: string): string => {
  const parts = filename.split('.');
  return parts.length > 1 ? parts[parts.length - 1] : '';
};

const FileItem: Component<{
  node: FileNode;
  depth: number;
  onFileClick: (path: string) => void;
}> = (props) => {
  const [isOpen, setIsOpen] = createSignal(false);
  const paddingLeft = () => `${props.depth * 12 + 8}px`;

  return (
    <Show
      when={props.node.is_dir}
      fallback={
        <button
          class="flex items-center w-full px-2 py-1 rounded cursor-pointer hover:bg-neutral-800 text-neutral-300 text-sm"
          style={{ "padding-left": paddingLeft() }}
          onClick={() => props.onFileClick(props.node.path)}
        >
          <FileIcon extension={getExtension(props.node.name)} />
          <span class="truncate">{props.node.name}</span>
        </button>
      }
    >
      <Collapsible.Root open={isOpen()} onOpenChange={({ open }) => setIsOpen(open)}>
        <Collapsible.Trigger
          class="flex items-center w-full px-2 py-1 rounded cursor-pointer hover:bg-neutral-800 text-neutral-300 text-sm"
          style={{ "padding-left": paddingLeft() }}
        >
          <ChevronIcon open={isOpen()} />
          <FolderIcon open={isOpen()} />
          <span class="truncate">{props.node.name}</span>
        </Collapsible.Trigger>
        <Collapsible.Content>
          <For each={props.node.children}>
            {(child) => (
              <FileItem
                node={child}
                depth={props.depth + 1}
                onFileClick={props.onFileClick}
              />
            )}
          </For>
        </Collapsible.Content>
      </Collapsible.Root>
    </Show>
  );
};

const FileTree: Component<{
  title: string;
  files: FileNode[];
  onFileClick: (path: string) => void;
  loading?: boolean;
}> = (props) => {
  return (
    <div class="mb-4">
      <div class="px-3 py-2 text-xs font-semibold text-neutral-500 uppercase tracking-wide">
        {props.title}
      </div>
      <Show
        when={!props.loading}
        fallback={
          <div class="px-3 py-2 text-neutral-500 text-sm">Loading...</div>
        }
      >
        <Show
          when={props.files.length > 0}
          fallback={
            <div class="px-3 py-2 text-neutral-600 text-sm italic">No files</div>
          }
        >
          <div class="px-1">
            <For each={props.files}>
              {(node) => (
                <FileItem node={node} depth={0} onFileClick={props.onFileClick} />
              )}
            </For>
          </div>
        </Show>
      </Show>
    </div>
  );
};

const filesToNodes = (files: FileEntry[]): FileNode[] => {
  return files.map((file) => ({
    id: file.path,
    name: file.name,
    path: file.path,
    is_dir: file.is_dir,
    children: file.is_dir ? [] : undefined,
  }));
};

export const FilesPanel: Component = () => {
  const { currentProject } = useProject();
  const { openFile } = useEditor();
  const [workspaceFiles, setWorkspaceFiles] = createSignal<FileNode[]>([]);
  const [kilnFiles, setKilnFiles] = createSignal<FileNode[]>([]);
  const [loadingWorkspace, setLoadingWorkspace] = createSignal(false);
  const [loadingKiln, setLoadingKiln] = createSignal(false);

  createEffect(async () => {
    const project = currentProject();
    if (!project) {
      setWorkspaceFiles([]);
      setKilnFiles([]);
      return;
    }

    setLoadingWorkspace(true);
    try {
      const files = await listFiles(project.path);
      setWorkspaceFiles(filesToNodes(files));
    } catch (err) {
      console.error('Failed to load workspace files:', err);
      setWorkspaceFiles([]);
    } finally {
      setLoadingWorkspace(false);
    }

    if (project.kilns.length > 0) {
      setLoadingKiln(true);
      try {
        const notes = await listKilnNotes(project.kilns[0]);
        setKilnFiles(filesToNodes(notes));
      } catch (err) {
        console.error('Failed to load kiln notes:', err);
        setKilnFiles([]);
      } finally {
        setLoadingKiln(false);
      }
    } else {
      setKilnFiles([]);
    }
  });

  const handleFileClick = (path: string) => {
    openFile(path);
  };

  return (
    <div class="h-full flex flex-col bg-neutral-900 text-neutral-100 overflow-hidden">
      <div class="p-3 border-b border-neutral-800 shrink-0">
        <h2 class="text-sm font-semibold text-neutral-400 uppercase tracking-wide">Files</h2>
      </div>

      <div class="flex-1 overflow-y-auto py-2">
        <Show
          when={currentProject()}
          fallback={
            <div class="px-3 py-8 text-center text-neutral-500 text-sm">
              Select a project to browse files
            </div>
          }
        >
          <FileTree
            title="Workspace"
            files={workspaceFiles()}
            onFileClick={handleFileClick}
            loading={loadingWorkspace()}
          />

          <FileTree
            title="Kiln"
            files={kilnFiles()}
            onFileClick={handleFileClick}
            loading={loadingKiln()}
          />
        </Show>
      </div>
    </div>
  );
};
