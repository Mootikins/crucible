import { copyText } from './clipboard';
import { notificationActions } from '@/stores/notificationStore';
import { openFileInEditor, fileOpenOptionsForEvent, type FileOpenOptions } from './file-actions';
import { kilnForElement, openNoteInEditor } from '@/lib/note-actions';

/**
 * Click delegation for rendered-markdown containers. Chat transcripts and the
 * note reading view share one implementation so their link semantics can't
 * drift: `[data-copy]` buttons copy the adjacent code block, `[data-note]`
 * anchors (wikilinks) open notes, external links open a new tab, and other
 * relative hrefs are treated as kiln note references.
 *
 * The kiln is read from the DOM — the nearest `data-kiln` ancestor of the
 * clicked link — rather than passed in. Hover reads it from the same element,
 * so the two cannot disagree about which kiln a link belongs to; when the kiln
 * was a parameter here and an attribute there, they agreed only because every
 * surface remembered to set both, and one did not.
 * between clicks.
 */
export function makeMarkdownClickHandler(onFollow?: (target: string, options: FileOpenOptions) => void, canFollow: () => boolean = () => !!onFollow): (event: MouseEvent) => void {
  return (event: MouseEvent) => {
    const target = event.target as HTMLElement | null;

    const copyBtn = target?.closest?.('[data-copy]');
    if (copyBtn) {
      event.preventDefault();
      const pre = copyBtn.closest('.md-codeblock')?.querySelector('pre');
      const code = pre?.textContent ?? '';
      if (code) {
        const prev = copyBtn.textContent;
        void copyText(code).then(() => {
          copyBtn.textContent = 'Copied';
          copyBtn.classList.add('is-copied');
          setTimeout(() => {
            copyBtn.textContent = prev;
            copyBtn.classList.remove('is-copied');
          }, 1200);
        }).catch(() => {
          notificationActions.addNotification('error', 'Copy was blocked. Select the text and copy it manually.');
        });
      }
      return;
    }

    const noteElement = target?.closest('[data-note]') as HTMLElement | null;
    if (noteElement) {
      event.preventDefault();
      const note = noteElement.dataset.note;
      if (note) {
        if (canFollow()) onFollow?.(note, fileOpenOptionsForEvent(event));
        else void openNoteInEditor(note, kilnForElement(noteElement));
      }
      return;
    }

    const anchor = target?.closest('a') as HTMLAnchorElement | null;
    if (!anchor) return;
    const href = anchor.getAttribute('href') ?? '';
    if (!href || href.startsWith('#')) return;
    event.preventDefault();
    if (/^[a-z][a-z0-9+.-]*:/i.test(href) || href.startsWith('//')) {
      window.open(href, '_blank', 'noopener,noreferrer');
      return;
    }
    let path = href;
    try { path = decodeURIComponent(href); } catch { /* A literal percent is a valid filename. */ }
    if (path.startsWith('/')) {
      const location = /^(.*):([1-9]\d*)$/.exec(path);
      const options = fileOpenOptionsForEvent(event);
      // The click reaches its pane's focus handler only after this handler.
      // A note owns in-place navigation; transcript links have no editor to replace.
      const tabId = anchor.closest<HTMLElement>('[data-file-tab-id]')?.dataset.fileTabId;
      if (tabId) options.tabId = tabId;
      else if (options.where === 'here') delete options.where;
      if (location) {
        path = location[1];
        options.line = Number(location[2]);
      }
      void openFileInEditor(path, undefined, options);
      return;
    }
    const note = path
      .replace(/^\.?\//, '')
      .replace(/\.md$/i, '');
    if (canFollow()) onFollow?.(note, fileOpenOptionsForEvent(event));
    else void openNoteInEditor(note, kilnForElement(anchor));
  };
}
