import { createSignal } from 'solid-js';
import type { ChatMode, ContextUsage } from '@/lib/types';

// ── Global status bar state ──────────────────────────────────────────────
// StatusBar lives outside ChatProvider/SessionProvider, so it can't use
// context hooks. This module-level store is updated by ChatContext when
// events arrive, and read by StatusBar directly.

const [chatMode, setChatMode] = createSignal<ChatMode>('normal');
const [contextUsage, setContextUsage] = createSignal<ContextUsage | null>(null);
const [activeModel, setActiveModel] = createSignal<string | null>(null);
const [notificationCount, setNotificationCount] = createSignal(0);
const [showThinking, setShowThinking] = createSignal(true);  // Toggle visibility of thinking blocks

export const statusBarStore = {
  chatMode,
  contextUsage,
  activeModel,
  notificationCount,
  showThinking,
} as const;

export const statusBarActions = {
  setChatMode,
  setContextUsage,
  setActiveModel,
  setNotificationCount,
  setShowThinking,
} as const;
