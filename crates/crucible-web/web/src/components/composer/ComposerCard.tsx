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
  /** Trailing controls, left of the mic — model/mode/agent pickers. */
  chips?: JSX.Element;
  /** The commit button — send, or cancel mid-stream. */
  action: JSX.Element;
}

/**
 * The shared composer: prompt, completion list, voice input and the commit
 * button.
 *
 * ONE ROW, not two. The prompt and every control that acts on it sit on a
 * single baseline-aligned line, and the row WRAPS when the pane is too narrow
 * to hold both — which is why this is `flex-wrap` and not a media or container
 * query. The previous shape stacked a fixed 2.5rem prompt above a separate
 * chip row, so an empty composer was 95px tall with ~30px of dead field
 * between the placeholder and the controls. It is now one line tall when
 * empty and grows only when there is text to hold.
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
  /**
   * Whether the prompt has grown past a single line.
   *
   * It decides where the controls sit. Sharing the line with a one-line prompt
   * costs nothing; sharing it with a three-line one costs the prompt ~40% of
   * its width for the whole paragraph, which is a worse trade than one extra
   * row of chrome. So the cluster drops to its own line the moment the prompt
   * wraps, and comes back up when it does not.
   */
  const [multiline, setMultiline] = createSignal(false);
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
    const content = el.scrollHeight;
    el.style.height = `${Math.min(content, MAX_HEIGHT_PX)}px`;
    // Compare against the field's OWN line height rather than a constant:
    // `rows` differs per surface (the launchpad opens at three) and the
    // reading size is a token that can move.
    const line = parseFloat(getComputedStyle(el).lineHeight) || 18;
    setMultiline(content > line * ((props.rows ?? 1) + 0.5));
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
        // `px` steps up with the radius: at one line the ends are 22px arcs,
        // and content set 14px from the edge collides with the curve. Squared
        // off there is no arc to clear, so the padding comes back down.
        classList={{
          'composer-surface flex flex-wrap items-end gap-x-2 gap-y-1 py-2 transition-[padding] duration-150': true,
          'px-4': !multiline(),
          'px-2.5': multiline(),
        }}
        data-multiline={multiline()}
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
          // `min-w` is what makes the wrap threshold meaningful: below it the
          // controls move to their own line instead of squeezing the prompt
          // down to a few characters.
          //
          // `outline-none` with NO `focus-ring` is deliberate and is the one
          // exception index.css records: the card draws the focus treatment
          // for this control, and a second ember ring 2px outside the first
          // is what the single-treatment rule exists to prevent.
          class="min-w-[11rem] flex-1 resize-none self-center bg-transparent px-1 py-0.5
                 text-sm leading-[1.55] text-shell-ink outline-none
                 placeholder-muted-dark disabled:opacity-50"
          style={{
            'min-height': `calc(${props.rows ?? 1} * 1.55em)`,
            'max-height': `${MAX_HEIGHT_PX}px`,
          }}
          data-testid={props.testid}
        />

        {/* Everything that acts on the prompt, pinned to the trailing edge.
            `ml-auto` holds it right whether it shares the prompt's line or
            takes one of its own. */}
        <div
          classList={{
            'ml-auto flex min-w-0 items-center gap-1': true,
            // `basis-full` is what forces the wrap; the flex container is
            // already `flex-wrap`, so nothing else has to change.
            'basis-full justify-end': multiline(),
          }}
        >
          {/* The pickers are the only part of the cluster allowed to shrink.
              Without this they held their full width in a narrow pane and
              pushed SEND out past the card's edge — the commit button, the
              one control the composer exists for, was unreachable at any pane
              width under ~260px. A model id truncates instead; ChipSelect
              already draws it with `truncate`. */}
          <div class="flex min-w-0 shrink items-center gap-1 overflow-hidden">
            {props.chips}
          </div>
          <MicButton
            onTranscription={handleTranscription}
            disabled={props.disabled}
            startRecording={startRecording}
            stopRecording={stopRecording}
            isRecording={isRecording}
          />
          {props.action}
        </div>
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
