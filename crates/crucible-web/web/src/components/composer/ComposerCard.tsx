import { Accessor, Component, JSX, Setter, Show, createEffect, createSignal } from 'solid-js';
import { MicButton } from '@/components/MicButton';
import { AutocompletePopup } from '@/components/AutocompletePopup';
import { useAutocomplete } from '@/hooks/useAutocomplete';
import { useMediaRecorder } from '@/hooks/useMediaRecorder';

/** Tallest the prompt grows before it scrolls inside itself. */
const MAX_HEIGHT_PX = 160;

export interface ComposerCardProps {
  value: Accessor<string>;
  setValue: Setter<string>;
  /**
   * Kiln backing `[[note]]`, `@file` and `#tag` completion. Absent (or null)
   * leaves those triggers inert; `/command` completion works regardless.
   */
  kilnPath?: Accessor<string | null | undefined>;
  placeholder: string;
  ariaLabel?: string;
  /** Lines the prompt occupies when empty. It grows from here, never below. */
  rows?: number;
  disabled?: boolean;
  /** `data-testid` for the textarea. */
  testid: string;
  onSubmit: () => void;
  /**
   * Extra key handling, run after the completion popup has had its chance and
   * before the Enter-to-submit default (so a handler can claim Enter).
   */
  onKeyDown?: (e: KeyboardEvent) => void;
  /** Pickers for the row below the prompt — model, mode, agent. */
  chips?: JSX.Element;
  /**
   * The quietest content of the row below, after the mic — session scope on
   * the live composer. It comes last because it changes least.
   */
  trailing?: JSX.Element;
  /** The commit button — send, or cancel mid-stream. */
  action: JSX.Element;
  /**
   * A card sits directly on top of the prompt (a pending permission or ask).
   * The prompt drops its top corners so the two read as one surface.
   */
  docked?: boolean;
}

/**
 * The shared composer: the prompt and its commit button in one surface, with
 * every picker in a quiet row below it.
 *
 * The prompt holds ONLY the text and the button. Chips inside the field made
 * the field look like a toolbar: the model id, the mode and the mic all sat
 * on the prompt's own line, so the loudest control on the screen carried five
 * competing labels. They now sit under the field, on the row that already
 * held the session scope, where they read as settings rather than as part of
 * the message.
 *
 * The card is also the completion list's ANCHOR. The list used to hang off the
 * textarea, which put a full-width panel over the last thing the agent said,
 * at a width unrelated to anything on screen. Anchored here it docks directly
 * above the field at exactly the field's width, the way the reference surfaces
 * dock theirs.
 *
 * The in-session chat input and the new-session launchpad share this. They
 * were byte-identical copies down to the class list, differing only in their
 * chips and commit button — and the copies had already diverged in behaviour:
 * only one of them wired up completion.
 */
export const ComposerCard: Component<ComposerCardProps> = (props) => {
  const [textareaRef, setTextareaRef] = createSignal<HTMLTextAreaElement | undefined>();
  const [cardRef, setCardRef] = createSignal<HTMLDivElement | undefined>();
  const { isRecording, audioLevel, startRecording, stopRecording } = useMediaRecorder();


  const autocomplete = useAutocomplete({
    input: props.value,
    setInput: props.setValue,
    kilnPath: props.kilnPath ?? (() => null),
    textareaRef,
  });

  /**
   * Grow the prompt to its content, then stop.
   *
   * `height: auto` first so `scrollHeight` reports the content height rather
   * than the height already set — without it the field only ever grows. The
   * floor comes from the CSS `min-height` (derived from `rows`) and the
   * ceiling from `max-height`, so this only has to write the middle.
   */
  const resize = (el: HTMLTextAreaElement) => {
    el.style.height = 'auto';
    el.style.height = `${Math.min(el.scrollHeight, MAX_HEIGHT_PX)}px`;
  };

  // Re-fit on EVERY value change, not just on keystrokes: a send clears the
  // prompt, a transcription appends a paragraph, and a draft restores one.
  // None of those are input events, and each leaves the field the wrong size.
  createEffect(() => {
    props.value();
    const el = textareaRef();
    if (el) resize(el);
  });

  const handleKeyDown = (e: KeyboardEvent) => {
    // The popup gets first refusal: Enter/Tab accept a completion rather than
    // sending a half-typed message.
    autocomplete.onKeyDown(e);
    if (e.defaultPrevented) return;

    props.onKeyDown?.(e);
    if (e.defaultPrevented) return;

    if (e.key === 'Enter' && !e.shiftKey) {
      e.preventDefault();
      props.onSubmit();
    }
  };

  const handleTranscription = (text: string) =>
    props.setValue((prev) => (prev.trim() ? `${prev} ${text}` : text));

  // While recording, the card fills bottom-up with the input level.
  const cardStyle = () => {
    if (!isRecording()) return {};
    const fill = Math.round(audioLevel() * 100);
    return {
      background: `linear-gradient(to top,
        color-mix(in srgb, var(--color-primary) 40%, transparent) 0%,
        color-mix(in srgb, var(--color-primary) 20%, transparent) ${fill}%,
        transparent ${fill}%)`,
      'border-color': 'color-mix(in srgb, var(--color-primary) 60%, transparent)',
    };
  };

  return (
    <div class="relative">
      <div
        ref={setCardRef}
        // ONE padding. The radius no longer changes with the line count, so
        // there is no arc for the content to clear at one height and not at
        // another. `data-docked` squares the top edge under a docked card;
        // both radii live in `refine-composer.css`, which is unlayered and
        // therefore wins over a Tailwind radius utility here.
        class="composer-surface flex items-end gap-x-2 px-3.5 py-2"
        data-docked={props.docked ? 'true' : undefined}
        style={cardStyle()}
      >
        <textarea
          ref={setTextareaRef}
          value={props.value()}
          onInput={(e) => {
            resize(e.currentTarget);
            void autocomplete.onInput(e);
          }}
          onKeyDown={handleKeyDown}
          onBlur={() => autocomplete.close()}
          placeholder={props.placeholder}
          aria-label={props.ariaLabel}
          rows={props.rows ?? 1}
          disabled={props.disabled}
          // `outline-none` with NO `focus-ring` is deliberate and is the one
          // exception index.css records: the card draws the focus treatment
          // for this control, and a second ember ring 2px outside the first
          // is what the single-treatment rule exists to prevent.
          class="min-w-0 flex-1 resize-none self-center bg-transparent px-1 py-0.5
                 text-sm leading-[1.55] text-shell-ink outline-none
                 placeholder-muted-dark disabled:opacity-50"
          style={{
            'min-height': `calc(${props.rows ?? 1} * 1.55em)`,
            'max-height': `${MAX_HEIGHT_PX}px`,
          }}
          data-testid={props.testid}
        />

        {/* The mic is an INPUT control, so it stays in the field beside
            send; the row below holds only session facts. */}
        <MicButton
          onTranscription={handleTranscription}
          disabled={props.disabled}
          startRecording={startRecording}
          stopRecording={stopRecording}
          isRecording={isRecording}
        />
        {props.action}
      </div>

      {/* The quiet row: the pickers, then whatever the surface considers its
          most stable fact (the session scope). Chips carry their value only;
          the icon and the tooltip name the role. The row wraps when a pane is
          narrower than the chips, so nothing hides behind a scroll edge. */}
      <div class="mt-1.5 flex flex-wrap items-center gap-x-1 gap-y-1" data-testid="composer-controls">
        {props.chips}
        {props.trailing}
      </div>

      <Show when={autocomplete.isOpen()}>
        <AutocompletePopup
          items={autocomplete.items()}
          selectedIndex={autocomplete.selectedIndex()}
          onSelect={(index) => autocomplete.complete(index)}
          anchor={cardRef()}
        />
      </Show>
    </div>
  );
};
