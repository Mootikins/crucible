import { Component, Show, onCleanup, createEffect } from 'solid-js';
import { X } from '@/lib/icons';
import { useExportSession, useSession } from '@/lib/query/sessions';

interface ExportDialogProps {
  open: boolean;
  sessionId: string | null;
  onClose: () => void;
}

/**
 * The rendered markdown of one session, to read or to download.
 *
 * The title is not passed in any more. It is a field of the session record,
 * and this dialog reads that record under the same key the pane that opened
 * it read, so a session renamed while the dialog is open names the file the
 * user downloads. The export itself is a mutation: the daemon renders it on
 * POST, nothing else reads the string, and it is dropped when the dialog
 * closes rather than cached.
 */
export const ExportDialog: Component<ExportDialogProps> = (props) => {
  const session = useSession(() => props.sessionId);
  const exportSession = useExportSession();

  const markdown = () => exportSession.data ?? '';
  const loading = () => exportSession.isPending;
  const error = () => (exportSession.error ? exportSession.error.message : null);

  // Render the export when the dialog opens on a session, and again when it
  // opens on a different one.
  createEffect(() => {
    if (!props.open) return;
    const sessionId = props.sessionId;
    if (!sessionId) return;
    exportSession.reset();
    exportSession.mutate(sessionId);
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
    const title = (session.data?.title ?? 'session')
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
            class="rounded-md bg-primary px-4 py-1.5 text-sm font-medium text-on-primary hover:bg-primary-hover disabled:opacity-40 disabled:cursor-not-allowed transition-colors"
          >
            Download
          </button>
        </div>
      </div>
    </Show>
  );
};
