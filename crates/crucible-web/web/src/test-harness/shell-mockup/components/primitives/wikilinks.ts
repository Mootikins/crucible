/**
 * Wikilinks inside rendered markdown. The markdown arrives as HTML, so the
 * events are delegated: one pair of handlers on an ancestor finds the
 * `[data-note]` link under the pointer. The caller resolves the target.
 */
export interface WikilinkEvents {
  onClick: (e: MouseEvent) => void;
  onMouseOver: (e: MouseEvent) => void;
}

export interface WikilinkIntent {
  /** A click on a link. `target` is the raw link text, not a resolved path. */
  open: (target: string, e: MouseEvent) => void;
  /** The pointer entered a link. */
  hover: (target: string, anchor: HTMLElement) => void;
}

const linkUnder = (e: Event) => (e.target as HTMLElement).closest<HTMLElement>('[data-note]');

export function wikilinkEvents(intent: WikilinkIntent): WikilinkEvents {
  return {
    onClick: (e) => {
      const a = linkUnder(e);
      if (!a) return;
      e.preventDefault();
      intent.open(a.dataset.note ?? '', e);
    },
    onMouseOver: (e) => {
      const a = linkUnder(e);
      if (a) intent.hover(a.dataset.note ?? '', a);
    },
  };
}
