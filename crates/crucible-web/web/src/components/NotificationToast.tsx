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
    border: 'border-primary/40',
    bg: 'bg-primary/15',
    icon: 'ℹ',
    iconColor: 'text-primary',
  },
  success: {
    border: 'border-ok/40',
    bg: 'bg-ok/15',
    icon: '✓',
    iconColor: 'text-ok',
  },
  warning: {
    border: 'border-attention/40',
    bg: 'bg-attention/15',
    icon: '⚠',
    iconColor: 'text-attention',
  },
  error: {
    border: 'border-error/40',
    bg: 'bg-error/15',
    icon: '✕',
    iconColor: 'text-error',
  },
};

// ── Individual Toast ─────────────────────────────────────────────────────

const Toast: Component<{
  id: string;
  type: NotificationType;
  message: string;
  action?: { label: string; run: () => void };
}> = (props) => {
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

      {/* Message + optional action */}
      <div class="flex-1 min-w-0">
        <p class="text-sm text-shell-ink break-words leading-snug">
          {props.message}
        </p>
        <Show when={props.action}>
          <button
            type="button"
            onClick={() => {
              props.action!.run();
              notificationActions.dismiss(props.id);
            }}
            class="mt-1.5 px-2.5 py-1 rounded border border-hairline-strong bg-control text-shell-ink text-xs font-medium hover:bg-hover-wash transition-colors"
          >
            {props.action!.label}
          </button>
        </Show>
      </div>

      {/* Dismiss button */}
      <button
        type="button"
        onClick={() => notificationActions.dismiss(props.id)}
        class="flex-shrink-0 text-muted-dark hover:text-shell-ink transition-colors p-0.5 -mr-1 -mt-0.5"
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
              <Toast id={notif.id} type={notif.type} message={notif.message} action={notif.action} />
            </div>
          )}
        </For>
      </div>
    </Show>
  );
};
