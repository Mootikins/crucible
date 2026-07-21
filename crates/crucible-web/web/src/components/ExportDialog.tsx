import { Component, Show, createSignal, onCleanup, createEffect } from 'solid-js';
import { X } from '@/lib/icons';
import { exportSession } from '@/lib/api';

interface ExportDialogProps {
  open: boolean;
  sessionId: string | null;
  sessionTitle: string | null;
  onClose: () => void;
}

export const ExportDialog: Component<ExportDialogProps> = (props) => {
  const [markdown, setMarkdown] = createSignal('');
  const [loading, setLoading] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  // Fetch markdown when dialog opens with a session
  createEffect(() => {
    if (props.open && props.sessionId) {
      setLoading(true);
      setError(null);
      setMarkdown('');

      exportSession(props.sessionId)
        .then((md) => {
          setMarkdown(md);
          setLoading(false);
        })
        .catch((err) => {
          setError(err instanceof Error ? err.message : 'Export failed');
          setLoading(false);
        });
    }
  });

  // Close on Escape
  createEffect(() => {
    if (!props.open) return;

    const onKeyDown = (e: KeyboardEvent) => {
      if (e.key === 'Escape') {
        e.preventDefault();
        e.stopPropagation();
        props.onClose();
      }
    };

    document.addEventListener('keydown', onKeyDown, true);
    onCleanup(() => document.removeEventListener('keydown', onKeyDown, true));
  });

  const previewLines = () => {
    const lines = markdown().split('\n');
    return lines.slice(0, 50).join('\n') + (lines.length > 50 ? '\n\n... (truncated)' : '');
  };

  const downloadFileName = () => {
    const title = (props.sessionTitle ?? 'session')
      .toLowerCase()
      .replace(/[^a-z0-9]+/g, '-')
      .replace(/^-|-$/g, '');
    const date = new Date().toISOString().slice(0, 10);
    return `session-${title}-${date}.md`;
  };

  const handleDownload = () => {
    const content = markdown();
    if (!content) return;

    const blob = new Blob([content], { type: 'text/markdown; charset=utf-8' });
    const url = URL.createObjectURL(blob);
    const a = document.createElement('a');
    a.href = url;
    a.download = downloadFileName();
    document.body.appendChild(a);
    a.click();
    document.body.removeChild(a);
    URL.revokeObjectURL(url);
  };

  return (
    <Show when={props.open}>
      {/* Backdrop */}
      <div
        class="fixed inset-0 z-[110] bg-black/65"
        onClick={() => props.onClose()}
      />

      {/* Dialog */}
      <div class="fixed left-1/2 top-16 z-[120] w-[min(720px,92vw)] max-h-[80vh] -translate-x-1/2 overflow-hidden rounded-xl border border-hairline bg-surface-overlay shadow-2xl backdrop-blur flex flex-col">
        {/* Header */}
        <div class="flex items-center justify-between border-b border-hairline px-5 py-3">
          <h2 class="text-sm font-semibold text-shell-ink">Export Session</h2>
          <button
            onClick={() => props.onClose()}
            class="rounded p-1 text-muted hover:bg-hover-wash hover:text-shell-ink transition-colors"
            aria-label="Close"
          >
            <X class="h-4 w-4" />
          </button>
        </div>

        {/* Content */}
        <div class="flex-1 overflow-y-auto p-5">
          <Show when={loading()}>
            <div class="flex items-center justify-center py-12">
              <div class="h-5 w-5 animate-spin rounded-full border-2 border-hairline border-t-shell-body" />
              <span class="ml-3 text-sm text-muted">Generating export...</span>
            </div>
          </Show>

          <Show when={error()}>
            <div class="rounded-lg border border-error/50 bg-error/10 p-4 text-sm text-error">
              {error()}
            </div>
          </Show>

          <Show when={!loading() && !error() && markdown()}>
            <div class="mb-3 flex items-center justify-between">
              <span class="text-xs text-muted-dark">
                Preview (first 50 lines) &middot; {markdown().split('\n').length} total lines
              </span>
              <span class="text-xs font-mono text-muted-dark">{downloadFileName()}</span>
            </div>
            <pre class="max-h-[50vh] overflow-y-auto rounded-lg border border-hairline bg-shell-bg p-4 text-xs leading-relaxed text-shell-body font-mono whitespace-pre-wrap">
              {previewLines()}
            </pre>
          </Show>
        </div>

        {/* Footer */}
        <div class="flex items-center justify-end gap-3 border-t border-hairline px-5 py-3">
          <button
            onClick={() => props.onClose()}
            class="rounded-md px-3 py-1.5 text-sm text-muted hover:bg-hover-wash hover:text-shell-ink transition-colors"
          >
            Close
          </button>
          <button
            onClick={handleDownload}
            disabled={!markdown() || loading()}
            class="rounded-md bg-primary px-4 py-1.5 text-sm font-medium text-white hover:bg-primary-hover disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            Download
          </button>
        </div>
      </div>
    </Show>
  );
};
