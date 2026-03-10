import { Component, For, Show, createSignal, createEffect, onCleanup } from 'solid-js';
import { notificationStore, notificationActions } from '@/stores/notificationStore';
import type { NotificationType } from '@/lib/types';

// ── Type-specific config ─────────────────────────────────────────────────

interface ToastStyle {
  border: string;
  bg: string;
  icon: string;
  iconColor: string;
}

const TOAST_STYLES: Record<NotificationType, ToastStyle> = {
  info: {
    border: 'border-blue-500/40',
    bg: 'bg-blue-950/80',
    icon: 'ℹ',
    iconColor: 'text-blue-400',
  },
  success: {
    border: 'border-emerald-500/40',
    bg: 'bg-emerald-950/80',
    icon: '✓',
    iconColor: 'text-emerald-400',
  },
  warning: {
    border: 'border-amber-500/40',
    bg: 'bg-amber-950/80',
    icon: '⚠',
    iconColor: 'text-amber-400',
  },
  error: {
    border: 'border-red-500/40',
    bg: 'bg-red-950/80',
    icon: '✕',
    iconColor: 'text-red-400',
  },
};

// ── Individual Toast ─────────────────────────────────────────────────────

const Toast: Component<{ id: string; type: NotificationType; message: string }> = (props) => {
  const [visible, setVisible] = createSignal(false);
  const style = () => TOAST_STYLES[props.type];

  // Slide in on mount
  createEffect(() => {
    const frame = requestAnimationFrame(() => setVisible(true));
    onCleanup(() => cancelAnimationFrame(frame));
  });

  return (
    <div
      class={`
        flex items-start gap-3 px-4 py-3 rounded-lg border backdrop-blur-sm
        shadow-lg shadow-black/30 max-w-sm w-full
        transition-all duration-300 ease-out
        ${style().border} ${style().bg}
        ${visible() ? 'translate-x-0 opacity-100' : 'translate-x-full opacity-0'}
      `}
      role="alert"
    >
      {/* Type icon */}
      <span class={`text-base font-bold flex-shrink-0 mt-0.5 ${style().iconColor}`}>
        {style().icon}
      </span>

      {/* Message */}
      <p class="text-sm text-neutral-200 flex-1 break-words leading-snug">
        {props.message}
      </p>

      {/* Dismiss button */}
      <button
        type="button"
        onClick={() => notificationActions.dismiss(props.id)}
        class="flex-shrink-0 text-neutral-500 hover:text-neutral-200 transition-colors p-0.5 -mr-1 -mt-0.5"
        aria-label="Dismiss notification"
      >
        <svg class="w-3.5 h-3.5" viewBox="0 0 16 16" fill="none" stroke="currentColor" stroke-width="2">
          <path d="M4 4l8 8M12 4l-8 8" />
        </svg>
      </button>
    </div>
  );
};

// ── Toast Container ──────────────────────────────────────────────────────

export const NotificationToast: Component = () => {
  const activeToasts = () =>
    notificationStore.notifications.filter((n) => !n.dismissed);

  return (
    <Show when={activeToasts().length > 0}>
      <div
        class="fixed bottom-14 right-4 z-50 flex flex-col-reverse gap-2 pointer-events-none"
        aria-live="polite"
        aria-label="Notifications"
      >
        <For each={activeToasts()}>
          {(notif) => (
            <div class="pointer-events-auto">
              <Toast id={notif.id} type={notif.type} message={notif.message} />
            </div>
          )}
        </For>
      </div>
    </Show>
  );
};
