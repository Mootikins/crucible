import { Component, For, Index, Show, createResource, createSignal } from 'solid-js';
import { ExternalLink, FileLock } from '@/lib/icons';
import {
  getConfig,
  saveConfig,
  type AppConfigNode,
  type Config,
  type ConfigOrigin,
} from '@/lib/api';
import { openFileAtLine } from '@/lib/file-actions';
import { notificationActions } from '@/stores/notificationStore';
import { useSettingsStack } from './settings-nav';
import { SettingsNavRow } from './MobileSettings';

/**
 * The daemon's own config, rendered from the daemon's own declaration.
 *
 * A GENERIC renderer, like `PluginSettings` next door and for the same reason:
 * the control tree is declared once — in Lua for a plugin, in Rust for the app
 * config — and every frontend draws it by switching on `type` alone. There is
 * no list of config keys in this file, so a key added to `CliAppConfig` with a
 * control descriptor appears here for free.
 *
 * This is the DAEMON's config, and it sits BESIDE the four browser-local
 * sections rather than replacing them. Fonts, terminal size, vim mode and the
 * microphone are per-device and stay in `localStorage`: a second device must
 * not inherit the first one's font. Nothing migrates.
 *
 * **A pinned key renders locked, and the lock has a route out.** A leaf the
 * user's `init.lua` holds is restored at the next boot, so a save into
 * `settings.json` would act nowhere — the daemon refuses it. Grafana, GNOME
 * dconf and VS Code all refuse such a write, and all three tell the user where
 * to change the value; a lock with no route out is a dead end. So the control
 * is disabled, it names the file and the line, and it offers to open that line.
 */

const inputClass =
  'w-56 max-w-full px-2 py-1 rounded border border-hairline bg-control text-shell-ink ' +
  'text-sm focus:outline-none focus:border-primary disabled:opacity-50';

/** The value at a dot-joined path, or undefined when no layer set it. */
function valueAt(config: Record<string, unknown> | undefined, path: string): unknown {
  let cursor: unknown = config;
  for (const segment of path.split('.')) {
    if (cursor === null || typeof cursor !== 'object') return undefined;
    cursor = (cursor as Record<string, unknown>)[segment];
  }
  return cursor;
}

/**
 * The config with one leaf replaced, cloned along the path it changed.
 *
 * The optimistic half of a save. A merge of the SAVE shape into the config
 * would not do: `{chat: {endpoint}}` replaces the whole `chat` subtree, and
 * every sibling would blink to its default until the daemon answered.
 */
function withValueAt(
  config: Record<string, unknown> | undefined,
  path: string,
  value: unknown,
): Record<string, unknown> {
  const segments = path.split('.');
  const leaf = segments.pop() as string;
  const root: Record<string, unknown> = { ...(config ?? {}) };
  let cursor = root;
  for (const segment of segments) {
    const held = cursor[segment];
    const next: Record<string, unknown> =
      held && typeof held === 'object' ? { ...(held as Record<string, unknown>) } : {};
    cursor[segment] = next;
    cursor = next;
  }
  cursor[leaf] = value;
  return root;
}

/** `chat.model` + value → `{chat: {model: value}}`, the shape a save takes. */
function nestValue(path: string, value: unknown): Record<string, unknown> {
  const segments = path.split('.');
  const leaf = segments.pop() as string;
  const root: Record<string, unknown> = {};
  let cursor = root;
  for (const segment of segments) {
    const next: Record<string, unknown> = {};
    cursor[segment] = next;
    cursor = next;
  }
  cursor[leaf] = value;
  return root;
}

/** The last path segment, for a file's display name. */
const basename = (path: string) => path.split('/').pop() || path;

/**
 * What a locked control says, in one place.
 *
 * The wording is deliberately careful about a HOST-CONDITIONAL pin: `init.lua`
 * may set a key inside a test on the hostname, so the same synced
 * `settings.json` is shadowed on one machine and not on another. The message
 * therefore says a line in the file sets the key here, and never that the user
 * asked for this value everywhere.
 */
const LockNote: Component<{ origin: ConfigOrigin; onJump: (origin: ConfigOrigin) => void }> = (
  props,
) => (
  <div class="mt-1 flex items-center gap-1.5 text-floor text-muted-dark" data-testid="config-lock">
    <FileLock class="h-3 w-3 flex-none text-primary" />
    <span>
      {props.origin.source === 'lua' || props.origin.source === 'toml'
        ? 'Set by a line in your config'
        : `Set by the daemon (${props.origin.source})`}
      <Show when={props.origin.file}>
        {' — '}
        <span data-testid="config-lock-source">
          {basename(props.origin.file as string)}
          <Show when={props.origin.line}>{`:${props.origin.line}`}</Show>
        </span>
      </Show>
      . That line runs again at every start, so a value saved here would never
      apply. The line may be conditional on this host, so another machine may
      show a different value.
    </span>
    <Show when={props.origin.file}>
      <button
        type="button"
        class="focus-ring inline-flex flex-none items-center gap-1 rounded border border-hairline
               px-1.5 py-0.5 text-floor text-shell-body hover:border-primary"
        data-testid="config-jump-to-pin"
        onClick={() => props.onJump(props.origin)}
      >
        <ExternalLink class="h-3 w-3" />
        Open the line
      </button>
    </Show>
  </div>
);

/** One leaf: its label, its lock if it has one, and its control. */
const AppConfigRow: Component<{
  node: AppConfigNode;
  effective?: Record<string, unknown>;
  origin?: ConfigOrigin;
  onSave: (path: string, value: unknown) => Promise<void>;
  onJump: (origin: ConfigOrigin) => void;
}> = (props) => {
  const [busy, setBusy] = createSignal(false);
  const path = () => props.node.path ?? props.node.key ?? '';
  const locked = () => props.origin?.pinned === true;
  const editable = () => !locked() && props.node.writable !== false && !busy();
  const value = () => {
    const held = valueAt(props.effective, path());
    return held === undefined || held === null ? props.node.default : held;
  };
  const text = () => {
    const held = value();
    return held === null || held === undefined ? '' : String(held);
  };

  const commit = async (next: unknown) => {
    setBusy(true);
    try {
      await props.onSave(path(), next);
    } finally {
      setBusy(false);
    }
  };

  return (
    <tr class="border-b border-hairline align-top" data-testid={`config-option-${path()}`}>
      <td class="py-3 pr-4">
        <div class="text-sm text-shell-body">{props.node.name ?? path()}</div>
        <Show when={props.node.desc}>
          <p class="mt-0.5 max-w-[34rem] text-floor leading-4 text-muted-dark">{props.node.desc}</p>
        </Show>
        <Show when={locked() && props.origin}>
          <LockNote origin={props.origin as ConfigOrigin} onJump={props.onJump} />
        </Show>
      </td>
      <td class="py-3 text-right">
        <Show when={props.node.type === 'toggle'}>
          <input
            type="checkbox"
            class="align-middle"
            checked={value() === true}
            disabled={!editable()}
            onChange={(e) => void commit(e.currentTarget.checked)}
          />
        </Show>

        <Show when={props.node.type === 'select'}>
          <select
            class={`cru-select ${inputClass}`}
            disabled={!editable()}
            value={text()}
            onChange={(e) => void commit(e.currentTarget.value)}
          >
            {/* An empty choice, so a value the daemon no longer offers reads as
                "not one of these" rather than silently as the first. */}
            <option value="">—</option>
            <For each={props.node.values ?? []}>
              {(choice) => (
                <option value={String(choice.value)} title={choice.desc}>
                  {choice.label}
                </option>
              )}
            </For>
          </select>
        </Show>

        <Show when={props.node.type === 'range'}>
          <input
            type="number"
            class={inputClass}
            min={props.node.min}
            max={props.node.max}
            step={props.node.step}
            value={text()}
            disabled={!editable()}
            onChange={(e) => {
              const raw = e.currentTarget.value;
              void commit(raw === '' ? null : Number(raw));
            }}
          />
        </Show>

        {/* `input`, `path`, `text`, and any kind added after this file was
            written. An unfamiliar kind renders plainly rather than vanishing:
            a newer daemon talking to a cached bundle is real skew, and a
            setting that is in effect must stay visible. */}
        <Show when={!['toggle', 'select', 'range'].includes(props.node.type)}>
          <input
            type="text"
            class={inputClass}
            value={text()}
            disabled={!editable()}
            onChange={(e) => {
              const raw = e.currentTarget.value;
              void commit(raw === '' ? null : raw);
            }}
          />
        </Show>
      </td>
    </tr>
  );
};

interface GroupProps {
  node: AppConfigNode;
  depth: number;
  effective?: Record<string, unknown>;
  origins: ConfigOrigin[];
  onSave: (path: string, value: unknown) => Promise<void>;
  onJump: (origin: ConfigOrigin) => void;
}

/** What a drill row promises, counted correctly at one. */
const countLabel = (n: number) => `${n} ${n === 1 ? 'setting' : 'settings'}`;

/** How many leaves a group holds, at any depth — what its row promises. */
function leafCount(node: AppConfigNode): number {
  return (node.args ?? []).reduce(
    (sum, child) => sum + (child.type === 'group' ? leafCount(child) : 1),
    0,
  );
}

/**
 * A group heading and its children, recursively.
 *
 * On a phone a child group becomes a ROW that opens its own page instead of a
 * heading over inlined rows. The daemon's config is the deepest tree the app
 * has, and inlining it gave a 412 px screen one scroll of every leaf the
 * daemon declares — the thing the drill-down exists to end. Nesting keeps
 * working: each pushed page renders at depth 0, so ITS groups drill too.
 */
const AppConfigGroup: Component<GroupProps> = (props) => {
  const stack = useSettingsStack();

  return (
    <>
      {/* The ROOT group draws no heading: the bar already names it. */}
      <Show when={props.depth > 0 && props.node.name}>
        <tr>
          <td
            colSpan={2}
            class="pt-5 pb-2 text-floor font-semibold uppercase tracking-wider text-muted-dark"
          >
            {props.node.name}
          </td>
        </tr>
      </Show>
      <Index each={props.node.args ?? []}>
        {(child) => (
          <Show
            when={child().type === 'group'}
            fallback={
              <AppConfigRow
                node={child()}
                effective={props.effective}
                origin={props.origins.find((row) => row.key === (child().path ?? child().key))}
                onSave={props.onSave}
                onJump={props.onJump}
              />
            }
          >
            <Show
              when={stack}
              fallback={
                <AppConfigGroup
                  node={child()}
                  depth={props.depth + 1}
                  effective={props.effective}
                  origins={props.origins}
                  onSave={props.onSave}
                  onJump={props.onJump}
                />
              }
            >
              {(nav) => (
                <tr class="border-b border-hairline">
                  <td colSpan={2} class="p-0">
                    <SettingsNavRow
                      label={child().name ?? child().key ?? 'Group'}
                      detail={countLabel(leafCount(child()))}
                      testId={`config-group-${child().key ?? child().name}`}
                      onSelect={() =>
                        nav().push({
                          id: `config:${child().path ?? child().key ?? child().name}`,
                          title: child().name ?? child().key ?? 'Group',
                          rows: true,
                          body: () => (
                            <AppConfigGroup
                              node={child()}
                              depth={0}
                              effective={props.effective}
                              origins={props.origins}
                              onSave={props.onSave}
                              onJump={props.onJump}
                            />
                          ),
                        })
                      }
                    />
                  </td>
                </tr>
              )}
            </Show>
          </Show>
        )}
      </Index>
    </>
  );
};

/**
 * The leaves that take no control at all, each with the daemon's reason.
 *
 * A location key names WHERE the daemon acts, and the RPC socket has no
 * authentication, so no browser control writes one. Showing the key with the
 * reason beats hiding it: a user hunting for `data_home` learns why it is not
 * here, instead of concluding the settings are incomplete.
 */
const ReadOnlyRows: Component<{
  rows: { path: string; reason: string }[];
  effective?: Record<string, unknown>;
}> = (props) => (
  <>
    <tr>
      <td
        colSpan={2}
        class="pt-5 pb-2 text-floor font-semibold uppercase tracking-wider text-muted-dark"
      >
        Read-only
      </td>
    </tr>
    <For each={props.rows}>
      {(row) => (
        <tr class="border-b border-hairline align-top" data-testid={`config-readonly-${row.path}`}>
          <td class="py-3 pr-4">
            <div class="text-sm text-shell-body">
              {row.path}
              <span class="ml-1 text-floor text-muted">(read-only)</span>
            </div>
            <p class="mt-0.5 max-w-[34rem] text-floor leading-4 text-muted-dark">{row.reason}</p>
          </td>
          <td class="py-3 text-right text-sm text-muted">
            {(() => {
              const held = valueAt(props.effective, row.path);
              return held === undefined || held === null || typeof held === 'object'
                ? '—'
                : String(held);
            })()}
          </td>
        </tr>
      )}
    </For>
  </>
);

/**
 * The app-config section of the settings modal.
 *
 * `onClose` is what makes the jump-to-pin useful: the file opens in the editor
 * behind the dialog, so the dialog has to go. A caller with no dialog (the
 * stacked settings tab) passes nothing and the jump still opens the tab.
 */
export const AppConfigSettingsSection: Component<{ onClose?: () => void }> = (props) => {
  const [answer, { mutate, refetch }] = createResource<Config>(() => getConfig());
  const [failure, setFailure] = createSignal<string | null>(null);

  const controls = () => answer()?.controls;
  const effective = () => answer()?.config;
  const origins = () => answer()?.origins ?? [];

  const jump = (origin: ConfigOrigin) => {
    if (!origin.file) return;
    props.onClose?.();
    openFileAtLine(origin.file, origin.line ?? 1, basename(origin.file));
  };

  const save = async (path: string, value: unknown) => {
    setFailure(null);
    // Optimistic, then reconciled against what the daemon actually holds: a
    // save can be refused per leaf, and the pane must end up showing the
    // stored value rather than what was typed at it.
    const previous = answer();
    mutate((held) => (held ? { ...held, config: withValueAt(held.config, path, value) } : held));
    try {
      const result = await saveConfig(nestValue(path, value));
      const refused = result.refused?.[0];
      if (refused) {
        const where = refused.file
          ? ` — ${basename(refused.file)}${refused.line ? `:${refused.line}` : ''} sets it`
          : '';
        setFailure(`${path} is not writable from here${where}.`);
        notificationActions.addNotification('error', `${path}: not saved${where}`);
      }
    } catch (err) {
      mutate(() => previous);
      setFailure(`${path}: ${err}`);
      notificationActions.addNotification('error', `${path}: ${err}`);
    }
    // Whatever happened, the daemon's answer replaces the guess — including
    // the provenance, which a save changes.
    await refetch();
  };

  return (
    <>
      <Show when={answer.loading && !answer()}>
        <tr>
          <td colSpan={2} class="py-3 text-center text-sm text-muted-dark">
            Loading configuration…
          </td>
        </tr>
      </Show>
      <Show when={answer.error || failure()}>
        <tr>
          <td colSpan={2} class="py-2 text-center text-xs text-error" data-testid="config-error">
            {failure() ?? String(answer.error)}
          </td>
        </tr>
      </Show>
      <Show when={controls()}>
        {(tree) => (
          <>
            <AppConfigGroup
              node={tree().options}
              depth={0}
              effective={effective()}
              origins={origins()}
              onSave={save}
              onJump={jump}
            />
            <Show when={(tree().read_only ?? []).length > 0}>
              <ReadOnlyRows rows={tree().read_only} effective={effective()} />
            </Show>
          </>
        )}
      </Show>
    </>
  );
};
