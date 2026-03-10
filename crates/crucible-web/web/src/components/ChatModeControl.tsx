import { Component } from 'solid-js';
import { useChatSafe } from '@/contexts/ChatContext';
import type { ChatMode } from '@/lib/types';

const MODES: { value: ChatMode; label: string }[] = [
  { value: 'normal', label: 'Normal' },
  { value: 'plan', label: 'Plan' },
  { value: 'auto', label: 'Auto' },
];

const MODE_ORDER: ChatMode[] = ['normal', 'plan', 'auto'];

/** Cycle to the next mode: Normal → Plan → Auto → Normal */
export function nextChatMode(current: ChatMode): ChatMode {
  const idx = MODE_ORDER.indexOf(current);
  return MODE_ORDER[(idx + 1) % MODE_ORDER.length];
}

export const ChatModeControl: Component = () => {
  const { chatMode, setChatMode } = useChatSafe();

  return (
    <div class="flex items-center rounded-lg border border-neutral-700 overflow-hidden">
      {MODES.map((mode) => (
        <button
          type="button"
          onClick={() => setChatMode(mode.value)}
          class="px-2.5 py-1 text-xs font-medium transition-colors outline-none"
          classList={{
            'bg-primary/80 text-white': chatMode() === mode.value,
            'text-neutral-400 hover:text-neutral-200 hover:bg-white/5': chatMode() !== mode.value,
          }}
          data-testid={`mode-${mode.value}`}
        >
          {mode.label}
        </button>
      ))}
    </div>
  );
};
