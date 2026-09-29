/**
 * The message box: the draft, then a row with attach, the context note, the
 * mode and the model, dictation and send. Enter sends; Shift+Enter makes a
 * new line.
 */
import { Show, type Component } from 'solid-js';
import { ChevronDown, Mic, Plus } from 'lucide-solid';
import { IconButton } from '../primitives/IconButton';
import { basename } from '../path';
import { ContextChip } from './ContextChip';
import { KnobButton } from './KnobButton';
import { SendButton } from './SendButton';

export interface ComposerProps {
  draft: string;
  onDraft: (text: string) => void;
  onSend: () => void;
  /** A turn runs: send queues the message. */
  running: boolean;
  /** The path of the note that goes with the message (`useSessionScopeChips` in the real app). */
  contextNote: string | null;
  onDropContext: (path: string) => void;
  mode: string;
  model: string;
}

export const Composer: Component<ComposerProps> = (props) => (
  <div class="mk-composer">
    <textarea
      rows={1}
      placeholder="Message"
      aria-label="Message"
      value={props.draft}
      onInput={(e) => props.onDraft(e.currentTarget.value)}
      onKeyDown={(e) => {
        if (e.key === 'Enter' && !e.shiftKey && !e.isComposing) {
          e.preventDefault();
          props.onSend();
        }
      }}
    />
    <div class="mk-crow">
      <IconButton label="Attach a note or a kiln">
        <Plus class="mk-i" />
      </IconButton>
      <Show when={props.contextNote}>{(n) => <ContextChip name={basename(n())} onRemove={() => props.onDropContext(n())} />}</Show>
      <span class="mk-grow" />
      <KnobButton title="Permission mode">{props.mode}</KnobButton>
      <KnobButton title="Model">
        {props.model}
        <ChevronDown class="mk-i" />
      </KnobButton>
      {/* The real app records through WhisperContext. */}
      <IconButton label="Hold to dictate">
        <Mic class="mk-i" />
      </IconButton>
      <SendButton running={props.running} disabled={!props.draft.trim()} onSend={props.onSend} />
    </div>
  </div>
);
