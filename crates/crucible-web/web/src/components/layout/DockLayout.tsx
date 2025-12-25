import { Component, ParentComponent } from 'solid-js';
import { DockView, DockPanel } from 'solid-dockview';
import 'dockview-core/dist/styles/dockview.css';

// Panel wrapper components for each content type
export const ChatPanel: ParentComponent = (props) => {
  return (
    <div class="h-full flex flex-col bg-neutral-900">
      {props.children}
    </div>
  );
};

export const PreviewPanel: Component = () => {
  return (
    <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
      <div class="text-center">
        <div class="text-4xl mb-2">📄</div>
        <div>Markdown Preview</div>
        <div class="text-sm text-neutral-600">Coming soon</div>
      </div>
    </div>
  );
};

export const EditorPanel: Component = () => {
  return (
    <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
      <div class="text-center">
        <div class="text-4xl mb-2">✏️</div>
        <div>Editor</div>
        <div class="text-sm text-neutral-600">Coming soon</div>
      </div>
    </div>
  );
};

export const CanvasPanel: Component = () => {
  return (
    <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
      <div class="text-center">
        <div class="text-4xl mb-2">🎨</div>
        <div>Canvas</div>
        <div class="text-sm text-neutral-600">Coming soon</div>
      </div>
    </div>
  );
};

export const GraphPanel: Component = () => {
  return (
    <div class="h-full flex items-center justify-center bg-neutral-900 text-neutral-500">
      <div class="text-center">
        <div class="text-4xl mb-2">🕸️</div>
        <div>Knowledge Graph</div>
        <div class="text-sm text-neutral-600">Coming soon</div>
      </div>
    </div>
  );
};

interface DockLayoutProps {
  chatContent: Component;
}

export const DockLayout: Component<DockLayoutProps> = (props) => {
  return (
    <DockView
      class="dockview-theme-abyss"
      style="height: 100vh; width: 100vw;"
    >
      <DockPanel id="chat" title="Chat">
        <props.chatContent />
      </DockPanel>
      <DockPanel id="preview" title="Preview">
        <PreviewPanel />
      </DockPanel>
      <DockPanel id="editor" title="Editor">
        <EditorPanel />
      </DockPanel>
    </DockView>
  );
};
