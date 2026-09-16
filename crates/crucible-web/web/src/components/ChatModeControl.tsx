import { Component, For, Show, createEffect, createSignal, onCleanup } from 'solid-js';
import { Portal } from 'solid-js/web';
import { useChatSafe } from '@/contexts/ChatContext';
import type { ChatMode, ModeDescriptor } from '@/lib/types';
import { placePopup } from '@/lib/popup-placement';
import {
  Check,
  CircleQuestionMark,
  Eye,
  Map,
  Pencil,
  Search,
  Shield,
  Sparkles,
  Wrench,
  Zap,
} from '@/lib/icons';

type IconComponent = Component<{ class?: string }>;

/**
 * The modes to offer before `session.list_modes` answers, and if it fails.
 *
 * Not a claim about what exists — the daemon's list replaces this wholesale.
 * It exists so the control is never empty, since a control with no modes
 * offers no way to change mode at all.
 */
export const FALLBACK_MODES: ModeDescriptor[] = [
  { id: 'ask', name: 'Ask', description: null, icon: null, color: null },
  { id: 'plan', name: 'Plan', description: null, icon: null, color: null },
  { id: 'auto', name: 'Auto', description: null, icon: null, color: null },
];

/**
 * The three built-in modes: a glyph and a description each.
 *
 * `runtime/defaults/init.luau` declares these modes with no description, so
 * the row would otherwise carry a bare name. A description the daemon DOES
 * send wins over the one here.
 */
const BUILTIN_MODES: Record<string, { icon: IconComponent; description: string }> = {
  ask: { icon: CircleQuestionMark, description: 'Asks before a tool changes anything' },
  plan: { icon: Map, description: 'Read-only tools; writes are denied' },
  auto: { icon: Zap, description: 'Runs tools without asking' },
};

/**
 * Icon names a Lua-declared mode may set in its `icon` field.
 *
 * A short, fixed list rather than the whole lucide set: an installed plugin
 * is operator code, but the name still has to map to a glyph this bundle
 * ships. An unknown name draws no icon, which is the Lua-only default.
 */
const NAMED_ICONS: Record<string, IconComponent> = {
  eye: Eye,
  pencil: Pencil,
  search: Search,
  shield: Shield,
  sparkles: Sparkles,
  wrench: Wrench,
  zap: Zap,
  map: Map,
  question: CircleQuestionMark,
};

/** The glyph for a mode, or `undefined` when it is Lua-only with no mapped icon. */
const iconFor = (mode: ModeDescriptor | undefined): IconComponent | undefined => {
  if (!mode) return undefined;
  return BUILTIN_MODES[mode.id]?.icon ?? (mode.icon ? NAMED_ICONS[mode.icon] : undefined);
};

const descriptionFor = (mode: ModeDescriptor): string | undefined =>
  mode.description ?? BUILTIN_MODES[mode.id]?.description;

/** Cycle to the next mode in the daemon's list, wrapping (Shift+Tab).
 *
 * A current mode absent from the list cycles nowhere: advancing into a mode
 * the daemon would reject leaves the control and the agent disagreeing. */
export function nextChatMode(current: ChatMode, available: readonly string[]): ChatMode {
  const idx = available.indexOf(current);
  if (idx === -1) return current;
  return available[(idx + 1) % available.length];
}

/** How long the list stays after the pointer leaves the circle or the list. */
const HOVER_CLOSE_DELAY_MS = 180;

/** Tallest the list gets before it scrolls inside itself. */
const PANEL_MAX_HEIGHT = 340;

/**
 * Chat-mode control: a circle the height of the chips beside it, wearing the
 * current mode's glyph, with a list of modes that opens under the pointer
 * and on click. Shift+Tab still cycles without opening.
 *
 * Hover opens because the circle carries no text: a reader who does not know
 * the glyph gets the list, with names and descriptions, by resting on it.
 * Click and the keyboard open it too, because hover is not reachable from a
 * keyboard or a touch screen.
 *
 * Modes are declared in Lua, so the list comes from the daemon rather than a
 * constant here. A built-in mode (ask/plan/auto) has a glyph and a
 * description; a Lua-declared mode is its name alone unless the daemon sent
 * an `icon` name this bundle maps.
 */
export const ChatModeControl: Component = () => {
  const { chatMode, switchMode, availableModes } = useChatSafe();
  const [open, setOpen] = createSignal(false);
  const [hover, setHover] = createSignal(-1);
  const [panelPos, setPanelPos] = createSignal<{
    left: number;
    top?: number;
    bottom?: number;
    maxHeight?: number;
  }>({ left: 0, top: 0 });
  let rootRef: HTMLDivElement | undefined;
  let triggerRef: HTMLButtonElement | undefined;
  let panelRef: HTMLDivElement | undefined;
  let closeTimer: ReturnType<typeof setTimeout> | undefined;

  const modes = () => availableModes();
  const current = () => modes().find((m) => m.id === chatMode());
  const currentName = () => current()?.name ?? chatMode();
  const CurrentIcon = () => iconFor(current());

  const positionPanel = () => {
    if (!triggerRef) return;
    const rect = triggerRef.getBoundingClientRect();
    const panel = panelRef?.getBoundingClientRect();
    setPanelPos(
      placePopup(rect, { width: window.innerWidth, height: window.innerHeight }, {
        width: Math.ceil(panel?.width || 240),
        preferredHeight: Math.ceil(panel?.height || PANEL_MAX_HEIGHT),
        gap: 4,
      }),
    );
  };

  const cancelClose = () => {
    if (closeTimer !== undefined) clearTimeout(closeTimer);
    closeTimer = undefined;
  };

  const openList = () => {
    cancelClose();
    if (open()) return;
    positionPanel();
    setHover(modes().findIndex((m) => m.id === chatMode()));
    setOpen(true);
  };

  const close = () => {
    cancelClose();
    setOpen(false);
    setHover(-1);
  };

  /** Close after a grace period, so the pointer can cross into the list. */
  const scheduleClose = () => {
    cancelClose();
    closeTimer = setTimeout(close, HOVER_CLOSE_DELAY_MS);
  };

  const pick = (mode: ModeDescriptor) => {
    switchMode(mode.id as ChatMode);
    close();
  };

  // Re-place once the panel exists and its real size is known (rAF, after
  // layout — a microtask would read the pre-layout estimate).
  createEffect(() => {
    if (!open()) return;
    requestAnimationFrame(positionPanel);
  });

  createEffect(() => {
    if (!open()) return;
    const onDocClick = (e: MouseEvent) => {
      const t = e.target as Node;
      if (rootRef && !rootRef.contains(t) && panelRef && !panelRef.contains(t)) close();
    };
    const onKey = (e: KeyboardEvent) => {
      const list = modes();
      if (e.key === 'Escape') {
        e.stopPropagation();
        close();
        triggerRef?.focus();
      } else if (e.key === 'ArrowDown' || e.key === 'ArrowUp') {
        e.preventDefault();
        if (!list.length) return;
        const delta = e.key === 'ArrowDown' ? 1 : -1;
        setHover((h) => (h + delta + list.length) % list.length);
      } else if (e.key === 'Enter' || e.key === ' ') {
        const mode = list[hover()];
        if (mode) {
          e.preventDefault();
          pick(mode);
        }
      }
    };
    const onViewportChange = () => close();
    document.addEventListener('mousedown', onDocClick);
    document.addEventListener('keydown', onKey);
    window.addEventListener('resize', onViewportChange);
    onCleanup(() => {
      document.removeEventListener('mousedown', onDocClick);
      document.removeEventListener('keydown', onKey);
      window.removeEventListener('resize', onViewportChange);
    });
  });

  onCleanup(cancelClose);

  return (
    <Show when={modes().length > 0}>
      <div ref={rootRef} class="relative inline-flex">
        <button
          ref={triggerRef}
          type="button"
          onMouseEnter={openList}
          onMouseLeave={scheduleClose}
          aria-label={`Mode: ${currentName()}`}
          title={`Mode: ${currentName()}`}
          aria-haspopup="listbox"
          aria-expanded={open()}
          data-testid="chat-mode"
          onClick={() => (open() ? close() : openList())}
          onKeyDown={(e) => {
            // The document handler takes over once the list is open; this
            // only has to open it. Enter and Space reach `onClick` natively.
            if (!open() && (e.key === 'ArrowDown' || e.key === 'ArrowUp')) {
              e.preventDefault();
              openList();
            }
          }}
          classList={{
            // The same height as the chips beside it (28px), drawn as a circle.
            'focus-ring flex h-7 w-7 aspect-square shrink-0 items-center justify-center rounded-full transition-colors': true,
            'text-shell-body hover:bg-hover-wash hover:text-shell-ink': !open(),
            'bg-hover-wash text-shell-ink': open(),
          }}
        >
          <Show
            when={CurrentIcon()}
            keyed
            // A Lua-only mode with no glyph shows its initial, so the circle
            // is never blank.
            fallback={
              <span class="text-floor font-medium uppercase leading-none" aria-hidden="true">
                {currentName().slice(0, 1)}
              </span>
            }
          >
            {(Icon) => <Icon class="h-3.5 w-3.5" aria-hidden="true" />}
          </Show>
        </button>

        <Show when={open()}>
          <Portal>
            <div
              ref={panelRef}
              data-testid="chat-mode-popout"
              role="listbox"
              aria-label="Mode"
              class="fixed z-50 flex flex-col min-w-[220px] max-w-[320px] overflow-y-auto bg-surface-overlay border border-hairline-strong rounded-lg shadow-xl py-1 cru-anim-rise"
              style={{
                left: `${panelPos().left}px`,
                ...(panelPos().top !== undefined
                  ? { top: `${panelPos().top}px` }
                  : { bottom: `${panelPos().bottom}px` }),
                'max-height': `${panelPos().maxHeight ?? PANEL_MAX_HEIGHT}px`,
              }}
              onMouseEnter={cancelClose}
              onMouseLeave={scheduleClose}
            >
              <For each={modes()}>
                {(mode, i) => {
                  const Icon = iconFor(mode);
                  const description = descriptionFor(mode);
                  return (
                    <button
                      type="button"
                      role="option"
                      aria-selected={mode.id === chatMode()}
                      onMouseEnter={() => setHover(i())}
                      onClick={() => pick(mode)}
                      data-testid={`mode-${mode.id}`}
                      classList={{
                        'w-full flex items-start gap-2 px-3 py-1.5 text-left text-xs text-shell-ink transition-colors': true,
                        'bg-hover-wash': hover() === i(),
                      }}
                    >
                      {/* Only a mode with a glyph gets the slot; a Lua-only
                          row starts at its name. */}
                      <Show when={Icon} keyed>
                        {(Glyph) => <Glyph class="mt-0.5 h-3.5 w-3.5 shrink-0 text-muted-dark" aria-hidden="true" />}
                      </Show>
                      <span class="flex min-w-0 flex-1 flex-col">
                        <span class="truncate">{mode.name}</span>
                        <Show when={description}>
                          <span class="text-floor leading-snug text-muted-dark">{description}</span>
                        </Show>
                      </span>
                      <Show when={mode.id === chatMode()}>
                        <Check class="mt-0.5 h-3.5 w-3.5 shrink-0 text-primary" aria-hidden="true" />
                      </Show>
                    </button>
                  );
                }}
              </For>
            </div>
          </Portal>
        </Show>
      </div>
    </Show>
  );
};
