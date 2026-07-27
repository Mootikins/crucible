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
export function makeMarkdownClickHandler(): (event: MouseEvent) => void {
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
      if (note) void openNoteInEditor(note, kilnForElement(noteElement));
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
    void openNoteInEditor(note, kilnForElement(anchor));
  };
}
