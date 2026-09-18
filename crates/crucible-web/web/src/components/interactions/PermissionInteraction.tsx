import { Component, For, Show, createSignal } from 'solid-js';
import type { InteractionOf, PermResponse, PermissionScope } from '@/lib/types';
import { DiffViewer } from '@/components/DiffViewer';
import { btnConsent, btnNeutral } from '@/lib/button-style';
import { deepPrettyPrintJson } from '@/lib/pretty-print';

interface Props {
  request: InteractionOf<'permission'>;
  onRespond: (response: PermResponse) => void;
}

// Tinted chip per action kind — the loud full-saturation bar (bg-attention
// slab) clashed with the UI's tinted vocabulary. A 15% fill + 50% border
// reads as the same family as btnPrimary/btnConsent while still flagging the
// action type at a glance.
const ACTION_LABELS: Record<string, { label: string; chip: string }> = {
  bash: { label: 'Execute', chip: 'bg-attention/15 text-attention border border-attention/50' },
  read: { label: 'Read', chip: 'bg-primary/15 text-primary border border-primary/50' },
  write: { label: 'Write', chip: 'bg-attention/15 text-attention border border-attention/50' },
  // Neutral: a tool name is a category, and precog means precognition only.
  tool: { label: 'Tool', chip: 'bg-surface-elevated text-muted border border-hairline-strong' },
};

/**
 * Full tool arguments as display pairs. A user must be able to see everything
 * they are approving — structured args (queries, URLs, nested objects) were
 * previously invisible unless mirrored into `tokens` (TUI parity:
 * `perm.full_commands`). No truncation; long values wrap.
 */
function toolArgPairs(request: InteractionOf<'permission'>): [string, string][] {
  if (request.action_type !== 'tool') return [];
  if (!request.tool_args || typeof request.tool_args !== 'object') return [];
  return Object.entries(request.tool_args as Record<string, unknown>).map(([k, v]) => [
    k,
    prettyPrintMaybeJson(v),
  ]);
}

/**
 * Render one value for the approval dialog. Delegates the decoding to the
 * shared `deepPrettyPrintJson` — same unwrapping as tool results, and
 * depth-capped, unlike a local re-parse loop. Expanding JSON nested inside
 * string fields matters most here: the user is approving whatever this box
 * shows, so an escaped one-line blob hides part of the request.
 */
function prettyPrintMaybeJson(raw: unknown): string {
  const decoded = deepPrettyPrintJson(raw);
  return typeof decoded === 'string' ? decoded : JSON.stringify(decoded, null, 2);
}

export const PermissionInteraction: Component<Props> = (props) => {
  const [scope, setScope] = createSignal<PermissionScope>('once');
  const [showScopes, setShowScopes] = createSignal(false);
  const [showDiff, setShowDiff] = createSignal(true);

  const actionInfo = () => ACTION_LABELS[props.request.action_type] || ACTION_LABELS.tool;

  // A tool request used to render a generic "Tool" chip AND a "Tool: <name>"
  // line beneath it — the word twice, the name once, and two lines spent on
  // one fact. The chip carries the tool's own name instead: it is the thing
  // that identifies the request, and "Permission Required" beside it already
  // says what kind of card this is. Non-tool actions keep their verb chip
  // ("Execute", "Read", "Write"), which is their identifying label.
  const isNamedTool = () =>
    props.request.action_type === 'tool' && !!props.request.tool_name;
  const chipLabel = () => (isNamedTool() ? props.request.tool_name! : actionInfo().label);
  // Two values, deliberately separate: the daemon's permission engine
  // pattern-matches against the EXACT token shape (`tokens.join(' ')`), so
  // the response payload must carry the raw pattern. The display can still
  // pretty-print the same string for human legibility — but the formatted
  // version must never reach `PermResponse.pattern`, or "Allow for session"
  // silently fails to grant (the persisted pattern won't match future
  // requests).
  const commandPattern = () => props.request.tokens.join(' ');
  const commandDisplay = () => prettyPrintMaybeJson(commandPattern());

  // The proposed edits as the daemon attached them to the request — the
  // authoritative change, with its true baseline. This page used to guess a
  // path out of `tokens[0]`, guess the new content out of `tool_args` field
  // names, and fetch the old content itself; all three guesses are gone.
  const requestDiffs = () => props.request.diffs ?? [];
  const hasDiff = () => requestDiffs().length > 0;

  const handleAllow = () => {
    props.onRespond({
      kind: 'permission',
      allowed: true,
      pattern: commandPattern(),
      scope: scope(),
    });
  };

  const handleDeny = () => {
    props.onRespond({
      kind: 'permission',
      allowed: false,
      scope: 'once',
    });
  };

  return (
    // Type sits at the transcript sizes: `text-floor` for the argument
    // listing (mono reads a step wider than prose), `text-xs` for the rest.
    // The card used to read a size above the turn it interrupts.
    <div class="bg-surface-elevated rounded-lg p-3 mb-4 border border-hairline">
      <div class="flex items-center gap-2 mb-2">
        <span
          class={`px-2 py-0.5 text-floor font-medium rounded-md ${actionInfo().chip} ${isNamedTool() ? 'font-mono' : ''}`}
          data-testid="perm-action-chip"
        >
          {chipLabel()}
        </span>
        <span class="text-floor uppercase tracking-wider text-muted-dark font-semibold">
          Permission Required
        </span>
      </div>

      {/* Full tool arguments — everything being approved must be visible */}
      <Show when={toolArgPairs(props.request).length > 0}>
        <div
          class="bg-surface-base rounded-md p-2 mb-3 max-h-40 overflow-y-auto font-mono text-floor leading-4 text-shell-ink"
          data-testid="perm-tool-args"
        >
          {toolArgPairs(props.request).map(([key, value]) => (
            <div class="whitespace-pre-wrap break-all">
              <span class="text-muted">{key}=</span>
              {value}
            </div>
          ))}
        </div>
      </Show>

      {/* Diff preview — the daemon's FileDiff projection, one viewer per
          file. No fetching: the request carries the change's true baseline. */}
      <Show when={hasDiff()}>
        <div class="mb-4">
          <button
            onClick={() => setShowDiff(!showDiff())}
            class="flex items-center gap-1 text-xs text-muted hover:text-shell-ink mb-2 transition-colors"
          >
            <span
              class="inline-block transition-transform duration-200"
              classList={{ 'rotate-90': showDiff() }}
            >
              ▶
            </span>
            {showDiff() ? 'Hide changes' : 'Show changes'}
          </button>
          <Show when={showDiff()}>
            {/* The diff is the only unbounded thing on the card, and the card
                now docks on the composer. A 500-line write must not push the
                Allow/Deny row off the bottom of the screen, so the preview
                scrolls and the decision stays where the user can reach it. */}
            <div class="max-h-64 overflow-y-auto">
              <For each={requestDiffs()}>
                {(d) => (
                  <DiffViewer
                    oldContent={d.old_content ?? ''}
                    newContent={d.new_content}
                    fileName={d.path}
                  />
                )}
              </For>
            </div>
          </Show>
        </div>
      </Show>

      {/* Fallback: show raw command text when neither a diff nor the
          tool-args block already covers the request */}
      <Show when={!hasDiff() && (commandDisplay() !== '' || toolArgPairs(props.request).length === 0)}>
        <div class="bg-surface-base rounded-md p-2 mb-3 font-mono text-floor leading-4 text-shell-ink overflow-x-auto">
          {commandDisplay() || '(no arguments)'}
        </div>
      </Show>

      {/* The two halves of a consent gate cost different amounts, so they must
          not look and reach alike.

          Deny comes FIRST — first in the reading order, first tab stop, one
          keystroke away — and wears the quiet neutral treatment. It was
          an error-tinted button, which painted the SAFE choice red: refusing an agent
          destroys nothing, and the red said otherwise.

          Allow comes second in `attention`, the theme's "this is waiting on
          your judgement" hue, not `btnPrimary`'s friendly confirm. This is the
          click that lets an agent run a command or write to disk; it should
          read as a decision, not as the OK button.

          Deliberately NOT autofocused. The card renders inline in a live
          transcript, and pulling focus off the composer would eat whatever the
          user was typing when the agent happened to stop. */}
      <div class="flex items-center gap-2 flex-wrap">
        <button onClick={handleDeny} class={btnNeutral} data-testid="perm-deny">
          Deny
        </button>
        <button onClick={handleAllow} class={btnConsent} data-testid="perm-allow">
          Allow
        </button>

        {/* Quieter than either: a disclosure, not a third choice. */}
        <button
          onClick={() => setShowScopes(!showScopes())}
          data-testid="perm-scopes-toggle"
          class="px-2 py-1.5 text-xs text-muted hover:text-shell-ink transition-colors"
        >
          {showScopes() ? 'Hide options' : 'More options...'}
        </button>
      </div>

      <Show when={showScopes()}>
        <div class="mt-3 pt-3 border-t border-hairline">
          <p class="text-muted text-xs mb-2">Allow for:</p>
          <div class="flex gap-1.5 flex-wrap">
            {(['once', 'session', 'project', 'user'] as PermissionScope[]).map((s) => (
              <button
                onClick={() => setScope(s)}
                classList={{
                  'px-2.5 py-1 text-floor rounded-md border transition-colors font-medium': true,
                  'bg-primary/15 text-primary border-primary/40': scope() === s,
                  'bg-surface-elevated text-shell-body border-hairline hover:bg-hover-wash': scope() !== s,
                }}
              >
                {s.charAt(0).toUpperCase() + s.slice(1)}
              </button>
            ))}
          </div>
        </div>
      </Show>
    </div>
  );
};
