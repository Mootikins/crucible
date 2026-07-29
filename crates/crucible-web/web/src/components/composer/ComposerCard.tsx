import { Accessor, Component, JSX, Setter, Show, createSignal } from 'solid-js';
import { MicButton } from '@/components/MicButton';
import { AutocompletePopup } from '@/components/AutocompletePopup';
import { useAutocomplete } from '@/hooks/useAutocomplete';
import { useMediaRecorder } from '@/hooks/useMediaRecorder';

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
  /** Footer row, left side — model/mode/agent pickers. */
  chips?: JSX.Element;
  /** Trailing button in the mic pill — send, or cancel mid-stream. */
  action: JSX.Element;
}

/**
 * The shared composer card: prompt box, completion popup, voice input, and the
 * mic/send pill.
 *
 * The in-session chat input and the new-session launchpad were byte-identical
 * copies of this markup down to the class list, differing only in their chips
 * and trailing button — and the copies had already diverged in behaviour: only
 * one of them wired up autocompletion. Owning the textarea here means a
 * completion fix lands on every surface at once.
 */
export const ComposerCard: Component<ComposerCardProps> = (props) => {
  const [textareaRef, setTextareaRef] = createSignal<HTMLTextAreaElement | undefined>();
  const { isRecording, audioLevel, startRecording, stopRecording } = useMediaRecorder();

  const autocomplete = useAutocomplete({
    input: props.value,
    setInput: props.setValue,
    kilnPath: props.kilnPath ?? (() => null),
    textareaRef,
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
    <div
      class="relative bg-surface-base border border-hairline-strong rounded-xl px-3 pt-2 pb-2 focus-within:border-primary transition-colors shadow-lg"
      style={cardStyle()}
    >
      <textarea
        ref={setTextareaRef}
        value={props.value()}
        onInput={(e) => void autocomplete.onInput(e)}
        onKeyDown={handleKeyDown}
        onBlur={() => autocomplete.close()}
        placeholder={props.placeholder}
        aria-label={props.ariaLabel}
        rows={props.rows ?? 1}
        disabled={props.disabled}
        class="w-full bg-transparent text-sm text-shell-ink placeholder-muted-dark resize-none outline-none px-1 py-1 max-h-32 min-h-[2.5rem] disabled:opacity-50"
        data-testid={props.testid}
      />
      <Show when={autocomplete.isOpen()}>
        <AutocompletePopup
          items={autocomplete.items()}
          selectedIndex={autocomplete.selectedIndex()}
          onSelect={(index) => autocomplete.complete(index)}
          anchor={textareaRef()}
        />
      </Show>

      <div class="flex items-center gap-1">
        {props.chips}
        <div class="flex-1" />
        {/* Mic and send share one pill, split by a hairline. */}
        <div class="flex items-stretch rounded-full border border-hairline overflow-hidden">
          <MicButton
            onTranscription={handleTranscription}
            disabled={props.disabled}
            startRecording={startRecording}
            stopRecording={stopRecording}
            isRecording={isRecording}
          />
          <div class="w-px bg-hairline" />
          {props.action}
        </div>
      </div>
    </div>
  );
};
