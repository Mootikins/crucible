import type { Component } from 'solid-js';

/** Names only: the daemon resolves the card in the session's final scope. */
export const AgentCardInput: Component<{
  value: string;
  onChange: (name: string) => void;
  disabled?: boolean;
}> = (props) => (
  <label class="flex items-center gap-2 text-xs text-muted-dark">
    Agent card
    <input
      aria-label="Agent card"
      class="rounded border border-border bg-transparent px-2 py-1 text-shell-ink focus-ring"
      placeholder="Default (optional card name)"
      value={props.value}
      disabled={props.disabled}
      onInput={(event) => props.onChange(event.currentTarget.value)}
    />
  </label>
);
