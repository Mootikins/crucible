// src/components/settings/primitives.tsx
//
// The row/section/debounce scaffolding shared by every settings subsection.
//
// Extracted from SettingsPanel.tsx so a second file can build sections without
// importing from the panel that renders it — SettingsPanel imports
// AdvancedSessionSettings, so the primitives living in SettingsPanel would have
// made that a module cycle. Solid components are plain functions, so the cycle
// would probably have resolved at render time; "probably" is not a reason to
// keep one.
import { Component, Show, type JSX } from 'solid-js';

export type IconComponent = Component<{ class?: string }>;

// =============================================================================
// Section Header
// =============================================================================

export const SectionHeader: Component<{ title: string; icon: IconComponent }> = (props) => (
  <tr>
    <td
      colSpan={2}
      class="pt-6 pb-2 text-xs font-semibold uppercase tracking-wider text-muted border-b border-hairline"
    >
      <span class="inline-flex items-center gap-1.5">
        <props.icon class="w-3.5 h-3.5" />
        {props.title}
      </span>
    </td>
  </tr>
);

// =============================================================================
// Reusable Setting Row Primitives
// =============================================================================

/** Full-width status row for loading, error, empty, and informational messages. */
export const StatusRow: Component<{ variant?: 'error'; children: JSX.Element }> = (props) => (
  <tr>
    <td
      colSpan={2}
      class={
        props.variant === 'error'
          ? 'py-2 text-center text-error text-xs'
          : 'py-3 text-center text-muted-dark text-sm'
      }
    >
      {props.children}
    </td>
  </tr>
);

/** Setting row with label, optional description, and a control cell. */
export const SettingRow: Component<{
  label: string;
  description?: string;
  controlClass?: string;
  children: JSX.Element;
}> = (props) => (
  <tr class="border-b border-hairline">
    <td class="py-3 text-shell-body text-sm">
      <div>{props.label}</div>
      <Show when={props.description}>
        <div class="text-xs text-muted-dark">{props.description}</div>
      </Show>
    </td>
    <td class={props.controlClass ?? 'py-3 text-right'}>
      {props.children}
    </td>
  </tr>
);

/**
 * Wrapper that handles the repeated async loading / error / no-session / empty
 * scaffolding shared by Model, Plugins, and MCP subsections.
 *
 * Pass `requiresSession` + `hasSession` for sections gated on an active session.
 * Pass `onRetry` to show a retry button alongside the error message.
 * Pass `hideContentOnError` to suppress children when an error is present (MCP).
 */
export const SettingsSectionState: Component<{
  title: string;
  icon: IconComponent;
  loading: boolean;
  error: string | null;
  loadingMessage?: string;
  requiresSession?: boolean;
  hasSession?: boolean;
  noSessionMessage?: string;
  isEmpty?: boolean;
  emptyMessage?: string;
  onRetry?: () => void;
  hideContentOnError?: boolean;
  children: JSX.Element;
}> = (props) => (
  <>
    <SectionHeader title={props.title} icon={props.icon} />

    <Show when={props.requiresSession && !props.hasSession}>
      <StatusRow>{props.noSessionMessage ?? 'No active session.'}</StatusRow>
    </Show>

    <Show when={!props.requiresSession || props.hasSession}>
      <Show when={props.loading}>
        <StatusRow>{props.loadingMessage ?? 'Loading…'}</StatusRow>
      </Show>

      <Show when={props.error}>
        <tr>
          <td colSpan={2} class="py-2">
            <div class="text-center text-error text-xs">{props.error}</div>
            <Show when={props.onRetry}>
              <div class="text-center mt-1">
                <button
                  onClick={props.onRetry}
                  class="px-2 py-1 text-xs rounded bg-control hover:bg-hover-wash text-shell-body transition-colors"
                >
                  Retry
                </button>
              </div>
            </Show>
          </td>
        </tr>
      </Show>

      <Show when={!props.loading && !(props.hideContentOnError && props.error)}>
        <Show when={props.isEmpty}>
          <StatusRow>{props.emptyMessage ?? 'No data available.'}</StatusRow>
        </Show>
        <Show when={!props.isEmpty}>
          {props.children}
        </Show>
      </Show>
    </Show>
  </>
);

// =============================================================================
// Debounce helper
// =============================================================================

export function createDebounce<T extends (...args: unknown[]) => void>(fn: T, delay: number) {
  let timer: ReturnType<typeof setTimeout> | null = null;
  const debounced = (...args: Parameters<T>) => {
    if (timer) clearTimeout(timer);
    timer = setTimeout(() => fn(...args), delay);
  };
  const cleanup = () => {
    if (timer) clearTimeout(timer);
  };
  return { debounced, cleanup };
}

// =============================================================================
// Model Settings Section
// =============================================================================

