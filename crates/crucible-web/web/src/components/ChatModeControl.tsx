import { Component } from 'solid-js';
import { useChatSafe } from '@/contexts/ChatContext';
import type { ChatMode } from '@/lib/types';

const MODES: { value: ChatMode; label: string; hint: string }[] = [
  { value: 'normal', label: 'Normal', hint: 'Agent acts directly' },
  { value: 'plan', label: 'Plan', hint: 'Agent drafts a plan before acting' },
  { value: 'auto', label: 'Auto', hint: 'Agent runs autonomously' },
];

const MODE_ORDER: ChatMode[] = ['normal', 'plan', 'auto'];

/** Cycle to the next mode: Normal → Plan → Auto → Normal */
export function nextChatMode(current: ChatMode): ChatMode {
  const idx = MODE_ORDER.indexOf(current);
  return MODE_ORDER[(idx + 1) % MODE_ORDER.length];
}

export const ChatModeControl: Component = () => {
  const { chatMode, switchMode } = useChatSafe();

  return (
    <div class="flex items-center rounded-lg border border-hairline overflow-hidden">
      {MODES.map((mode) => (
        <button
          type="button"
          onClick={() => switchMode(mode.value)}
          title={`${mode.label}: ${mode.hint} (Shift+Tab to cycle)`}
          class="px-2.5 py-1 text-xs font-medium transition-colors outline-none"
          classList={{
            'bg-primary/80 text-white': chatMode() === mode.value,
            'text-muted hover:text-shell-ink hover:bg-hover-wash': chatMode() !== mode.value,
          }}
          data-testid={`mode-${mode.value}`}
        >
          {mode.label}
        </button>
      ))}
    </div>
  );
};
