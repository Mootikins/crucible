import { Component } from 'solid-js';
import { useChatSafe } from '@/contexts/ChatContext';
import type { ChatMode } from '@/lib/types';
import { ChipSelect } from '@/components/composer/ChipSelect';

const MODES: { value: ChatMode; label: string; hint: string }[] = [
  { value: 'normal', label: 'Normal', hint: 'Agent acts directly' },
  { value: 'plan', label: 'Plan', hint: 'Agent drafts a plan before acting' },
  { value: 'auto', label: 'Auto', hint: 'Agent runs autonomously' },
];

const MODE_ORDER: ChatMode[] = ['normal', 'plan', 'auto'];

/** Cycle to the next mode: Normal → Plan → Auto → Normal (Shift+Tab) */
export function nextChatMode(current: ChatMode): ChatMode {
  const idx = MODE_ORDER.indexOf(current);
  return MODE_ORDER[(idx + 1) % MODE_ORDER.length];
}

/**
 * Chat-mode picker — the launchpad's ChipSelect dropdown idiom (was a
 * three-button segmented control). Shift+Tab still cycles without opening.
 */
export const ChatModeControl: Component = () => {
  const { chatMode, switchMode } = useChatSafe();

  return (
    <ChipSelect
      name="mode"
      options={MODES.map((m) => ({ value: m.value, label: m.label, hint: m.hint }))}
      value={chatMode()}
      onSelect={(v) => switchMode(v as ChatMode)}
      testid="chat-mode"
      optionTestidPrefix="mode"
    />
  );
};
