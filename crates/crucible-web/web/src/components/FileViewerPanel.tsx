import type { Component } from 'solid-js';

interface FileViewerPanelProps {
  filePath?: string;
}

const FileViewerPanel: Component<FileViewerPanelProps> = (props) => {
  return (
    <div class="h-full bg-neutral-900 p-4 flex items-center justify-center text-neutral-400 text-sm">
      File: {props.filePath || 'No file selected'}
    </div>
  );
};

export default FileViewerPanel;
