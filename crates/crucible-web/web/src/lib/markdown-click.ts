import { openNoteInEditor } from '@/lib/note-actions';

/**
 * Click delegation for rendered-markdown containers. Chat transcripts and the
 * note reading view share one implementation so their link semantics can't
 * drift: `[data-copy]` buttons copy the adjacent code block, `[data-note]`
 * anchors (wikilinks) open notes, external links open a new tab, and other
 * relative hrefs are treated as kiln note references.
 *
 * `kiln` is an accessor because the surface decides where notes resolve
 * (chat: the session's kiln; preview: the active kiln) and it can change
 * between clicks.
 */
export function makeMarkdownClickHandler(
  kiln: () => string | undefined,
): (event: MouseEvent) => void {
  return (event: MouseEvent) => {
    const target = event.target as HTMLElement | null;

    const copyBtn = target?.closest?.('[data-copy]');
    if (copyBtn) {
      event.preventDefault();
      const pre = copyBtn.closest('.md-codeblock')?.querySelector('pre');
      const code = pre?.textContent ?? '';
      if (code) {
        void navigator.clipboard?.writeText(code);
        const prev = copyBtn.textContent;
        copyBtn.textContent = 'Copied';
        copyBtn.classList.add('is-copied');
        setTimeout(() => {
          copyBtn.textContent = prev;
          copyBtn.classList.remove('is-copied');
        }, 1200);
      }
      return;
    }

    const noteElement = target?.closest('[data-note]') as HTMLElement | null;
    if (noteElement) {
      event.preventDefault();
      const note = noteElement.dataset.note;
      if (note) void openNoteInEditor(note, kiln());
      return;
    }

    const anchor = target?.closest('a') as HTMLAnchorElement | null;
    if (!anchor) return;
    const href = anchor.getAttribute('href') ?? '';
    if (!href || href.startsWith('#')) return;
    event.preventDefault();
    if (/^[a-z][a-z0-9+.-]*:/i.test(href)) {
      window.open(href, '_blank', 'noopener,noreferrer');
      return;
    }
    const note = decodeURIComponent(href)
      .replace(/^\.?\//, '')
      .replace(/\.md$/i, '');
    void openNoteInEditor(note, kiln());
  };
}
