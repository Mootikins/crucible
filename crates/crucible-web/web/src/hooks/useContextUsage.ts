import type { Accessor } from 'solid-js';
import type { ContextUsage } from '@/lib/types';
import { useChatSafe } from '@/contexts/ChatContext';

/**
 * Returns the current context window usage from ChatContext.
 * Safe to call outside ChatProvider — returns null via fallback.
 */
export function useContextUsage(): Accessor<ContextUsage | null> {
  const ctx = useChatSafe();
  return () => ctx.contextUsage();
}
