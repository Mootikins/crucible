import { Component, Show } from 'solid-js';
import { useChatSafe } from '@/contexts/ChatContext';
import type { ChatMode, ModeDescriptor } from '@/lib/types';
import { ChipSelect } from '@/components/composer/ChipSelect';

/**
 * The modes to offer before `session.list_modes` answers, and if it fails.
 *
 * Not a claim about what exists — the daemon's list replaces this wholesale.
 * It exists so the chip is never empty, since an empty ChipSelect renders as
 * a placeholder with no way to change mode at all.
 */
export const FALLBACK_MODES: ModeDescriptor[] = [
  { id: 'normal', name: 'Normal', description: null, icon: null, color: null },
  { id: 'plan', name: 'Plan', description: null, icon: null, color: null },
  { id: 'auto', name: 'Auto', description: null, icon: null, color: null },
];

/** Cycle to the next mode in the daemon's list, wrapping (Shift+Tab).
 *
 * A current mode absent from the list cycles nowhere: advancing into a mode
 * the daemon would reject leaves the chip and the agent disagreeing. */
export function nextChatMode(current: ChatMode, available: readonly string[]): ChatMode {
  const idx = available.indexOf(current);
  if (idx === -1) return current;
  return available[(idx + 1) % available.length];
}

/**
 * Chat-mode picker — the launchpad's ChipSelect dropdown idiom (was a
 * three-button segmented control). Shift+Tab still cycles without opening.
 *
 * Modes are declared in Lua, so the option list comes from the daemon rather
 * than a constant here: a session in a Lua-declared `review` used to fall
 * through to the placeholder because `review` was in no hardcoded list.
 */
export const ChatModeControl: Component = () => {
  const { chatMode, switchMode, availableModes } = useChatSafe();

  const options = () => availableModes().map((m) => ({ value: m.id, label: m.name }));

  return (
    <Show when={options().length > 0}>
      <ChipSelect
        name="mode"
        // Without this the trigger falls back to the literal string "mode"
        // whenever the current mode is absent from the options.
        placeholder={chatMode()}
        options={options()}
        value={chatMode()}
        onSelect={(v) => switchMode(v as ChatMode)}
        testid="chat-mode"
        optionTestidPrefix="mode"
      />
    </Show>
  );
};
