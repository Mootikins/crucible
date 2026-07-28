import { Component, createMemo, createSignal, onCleanup, onMount } from 'solid-js';
import { Show } from 'solid-js';

/**
 * Schemes a web card may be created with.
 *
 * The renderer refuses to navigate anything else anyway, but rejecting it here
 * means a `javascript:` URL never reaches the document in the first place —
 * better than writing one to disk and relying on every future reader to keep
 * treating it as inert.
 */
const ALLOWED_SCHEMES = ['http:', 'https:'];

/**
 * Normalise what a person actually types.
 *
 * Nobody types the scheme, so `jsoncanvas.org` has to become a URL rather than
 * an error. Assuming `https` is the safe direction of the two.
 */
export function normaliseUrl(raw: string): string | null {
  const trimmed = raw.trim();
  if (!trimmed) return null;

  // `word:` looks like a scheme — but so does `localhost:3000`, and reading
  // that port as a scheme rejected the most common way anyone writes a local
  // dev server. A colon followed by digits and nothing else is a port.
  const schemed =
    /^[a-z][a-z0-9+.-]*:/i.test(trimmed) &&
    !/^[a-z][a-z0-9+.-]*:\d+(?:[/?#]|$)/i.test(trimmed);
  const candidate = schemed ? trimmed : `https://${trimmed}`;
  let parsed: URL;
  try {
    parsed = new URL(candidate);
  } catch {
    return null;
  }
  if (!ALLOWED_SCHEMES.includes(parsed.protocol)) return null;
  // A bare word like "notes" parses as a URL with an empty host once prefixed;
  // that is a typo, not an address.
  if (!parsed.host) return null;
  return parsed.toString();
}

/** Prompt for a URL to drop onto the canvas as a web card. */
export const LinkPrompt: Component<{
  onSubmit: (url: string) => void;
  onClose: () => void;
}> = (props) => {
  const [value, setValue] = createSignal('');
  let input: HTMLInputElement | undefined;

  const normalised = createMemo(() => normaliseUrl(value()));
  const invalid = createMemo(() => value().trim().length > 0 && normalised() === null);

  let restoreFocusTo: HTMLElement | null = null;
  onMount(() => {
    restoreFocusTo = document.activeElement as HTMLElement | null;
    input?.focus();
  });
  onCleanup(() => restoreFocusTo?.focus());

  const submit = () => {
    const url = normalised();
    if (url) props.onSubmit(url);
  };

  return (
    <div
      class="absolute inset-0 z-20 flex items-start justify-center bg-black/30 pt-16"
      data-testid="canvas-link-prompt"
      onPointerDown={(e) => {
        if (e.target !== e.currentTarget) return;
        // Keep the backdrop click off the canvas, which would otherwise clear
        // the selection and start a marquee behind the closing dialog.
        e.stopPropagation();
        props.onClose();
      }}
    >
      <div
        role="dialog"
        aria-modal="true"
        aria-label="Add a web page"
        class="flex w-96 flex-col overflow-hidden rounded-lg border border-hairline bg-surface-elevated shadow-lg"
        onPointerDown={(e) => e.stopPropagation()}
      >
        <input
          ref={input}
          class="bg-transparent px-3 py-2 text-sm text-shell-ink outline-none"
          placeholder="https://…"
          value={value()}
          data-testid="canvas-link-prompt-input"
          onInput={(e) => setValue(e.currentTarget.value)}
          onKeyDown={(e) => {
            e.stopPropagation();
            if (e.key === 'Escape') props.onClose();
            if (e.key === 'Enter') submit();
            // The dialog announces `aria-modal`, so focus must not leave it.
            // The input is its only focusable control, which makes trapping a
            // matter of refusing Tab — without this a keyboard user tabbed
            // straight out onto the toolbar behind the backdrop and could fire
            // Undo or Delete while the "modal" was still open.
            if (e.key === 'Tab') e.preventDefault();
          }}
        />
        <Show when={invalid()}>
          <div
            class="border-t border-hairline px-3 py-1.5 text-[11px] text-error"
            data-testid="canvas-link-prompt-error"
          >
            Enter an http or https address.
          </div>
        </Show>
      </div>
    </div>
  );
};
