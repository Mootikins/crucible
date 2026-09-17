import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, waitFor, screen, fireEvent } from '@solidjs/testing-library';
import { createSignal } from 'solid-js';
import { createTestQueryEnv, type TestQueryEnv } from '@/test-utils/query';
import { PluginSettings } from '../PluginSettings';
import type { PluginOptionNode } from '@/lib/types';

const addNotification = vi.fn();
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...a: unknown[]) => addNotification(...a) },
}));

/**
 * One option route answers all three actions, the way the daemon's does.
 *
 * The pane used to be driven through stubs of `getPluginOption` and
 * `setPluginOption`, which could not see the thing this task changed: the value
 * is a cache entry now, and the write invalidates it. Reading the actual
 * requests is what makes "re-read once after a write" a statement about the
 * wire rather than about a stub's call count.
 */
interface OptionCall {
  action: string;
  path: string[];
  value?: unknown;
}

let env: TestQueryEnv;
let calls: OptionCall[];
/** The plugin's stored value, which its setter is free to normalise. */
let stored: unknown;
/** What the setter does with what it was given. */
let onSet: (value: unknown) => void;
/** Whether the setter refuses. */
let refusal: string | null;

const reads = () => calls.filter((call) => call.action === 'get').length;

beforeEach(() => {
  calls = [];
  stored = null;
  refusal = null;
  onSet = (value) => {
    stored = value;
  };
  env = createTestQueryEnv({
    'POST /api/plugins/oci/option': async (request: Request) => {
      const body = (await request.json()) as OptionCall;
      calls.push(body);
      if (body.action === 'get') return { value: stored };
      if (refusal) {
        return new Response(JSON.stringify({ error: { code: 422, message: refusal } }), {
          status: 422,
        });
      }
      if (body.action === 'set') onSet(body.value);
      return {};
    },
  });
});

afterEach(() => {
  cleanup();
  env?.restore();
  vi.clearAllMocks();
});

const tree = (args: PluginOptionNode[]): PluginOptionNode => ({ type: 'group', args });

const mount = (node: PluginOptionNode, onChanged = () => {}) =>
  render(() => <PluginSettings plugin="oci" tree={node} onChanged={onChanged} />);

describe('PluginSettings', () => {
  // The anti-regression test for the generic-rendering rule, the same one
  // SessionStatusChips carries. If a plugin ever needs a change in this file to
  // get a settings pane, the options channel stopped being generic.
  it('renders a tree from a plugin it has never heard of', async () => {
    stored = 'flux';
    mount(
      tree([
        { key: 'zarquon', type: 'input', name: 'Flux capacitor', desc: 'Charge level', writable: true },
      ]),
    );

    const row = await waitFor(() => screen.getByTestId('plugin-option-zarquon'));
    expect(row.textContent).toContain('Flux capacitor');
    expect(row.textContent).toContain('Charge level');
    await waitFor(() => expect(row.querySelector('input')).toHaveValue('flux'));
  });

  // Widget kinds are the plugin's word, not an enum this file owns. One added
  // after this file was written must still render — an invisible setting that
  // is nonetheless in effect is the worst outcome.
  it('renders an unrecognised widget kind as a value rather than nothing', async () => {
    stored = '42';
    mount(tree([{ key: 'novel', type: 'colour-wheel', name: 'Hue', writable: true }]));

    const row = await waitFor(() => screen.getByTestId('plugin-option-novel'));
    await waitFor(() => expect(row.querySelector('input')).toHaveValue('42'));
  });

  it('writes through the plugin setter and re-reads what was actually stored', async () => {
    // The setter normalises: what comes back is not what was typed, and the
    // pane must end up showing the stored value.
    stored = 'alpine';
    onSet = (value) => {
      stored = `docker.io/${String(value)}`;
    };
    mount(tree([{ key: 'image', type: 'input', name: 'Image', writable: true }]));

    const input = await waitFor(() =>
      screen.getByTestId('plugin-option-image').querySelector('input')!,
    );
    await waitFor(() => expect(input).toHaveValue('alpine'));
    fireEvent.change(input, { target: { value: 'debian' } });

    await waitFor(() =>
      expect(calls).toContainEqual({ action: 'set', path: ['image'], value: 'debian' }),
    );
    await waitFor(() => expect(input).toHaveValue('docker.io/debian'));
  });

  it('rolls back and reports when the plugin refuses a value', async () => {
    stored = 'alpine';
    refusal = 'unknown image';
    mount(tree([{ key: 'image', type: 'input', name: 'Image', writable: true }]));

    const input = await waitFor(() =>
      screen.getByTestId('plugin-option-image').querySelector('input')!,
    );
    await waitFor(() => expect(input).toHaveValue('alpine'));
    fireEvent.change(input, { target: { value: 'nope' } });

    await waitFor(() => expect(addNotification).toHaveBeenCalled());
    expect(addNotification.mock.calls[0][0]).toBe('error');
    // Not left showing a value the daemon never stored.
    await waitFor(() => expect(input).toHaveValue('alpine'));
  });

  // A write can change what a SIBLING offers — `values` and `disabled` are
  // functions evaluated against current state. Re-reading only the row that
  // changed would leave the rest of the pane describing the box as it was.
  it('invalidates the whole tree after a write, not just the row', async () => {
    const onChanged = vi.fn();
    stored = false;
    mount(tree([{ key: 'flag', type: 'toggle', name: 'Flag', writable: true }]), onChanged);

    const box = await waitFor(() =>
      screen.getByTestId('plugin-option-flag').querySelector('input')!,
    );
    fireEvent.change(box, { target: { checked: true } });

    // The new state, not merely "something was written" — a toggle that always
    // committed its old value would satisfy the invalidation check alone.
    await waitFor(() =>
      expect(calls).toContainEqual({ action: 'set', path: ['flag'], value: true }),
    );
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  // The write asks the plugin's options again, once. A pane that both
  // reconciled the row and reloaded the tree paid two round trips per commit
  // for one value, and the second could only ever return what the first did.
  it('reads a written option exactly once', async () => {
    const leaf: PluginOptionNode = { key: 'image', type: 'input', name: 'Image', writable: true };
    const [tree_, setTree] = createSignal(tree([leaf]));
    stored = 'alpine';
    onSet = () => {
      stored = 'docker.io/debian';
    };
    // What PluginPanel does on `onChanged`: re-read `plugin.options`, which
    // comes back off the wire as a fresh object graph every time.
    const onChanged = async () => setTree(tree([{ ...leaf }]));
    render(() => <PluginSettings plugin="oci" tree={tree_()} onChanged={onChanged} />);

    const input = await waitFor(() =>
      screen.getByTestId('plugin-option-image').querySelector('input')!,
    );
    await waitFor(() => expect(input).toHaveValue('alpine'));
    calls = [];

    fireEvent.change(input, { target: { value: 'debian' } });

    await waitFor(() => expect(input).toHaveValue('docker.io/debian'));
    // Asserting an absence: give a redundant read the chance to be issued
    // before counting, or this passes by arriving early.
    await new Promise((resolve) => setTimeout(resolve, 0));
    expect(reads()).toBe(1);
  });

  it('offers a select only the choices the daemon evaluated', async () => {
    stored = 'podman';
    mount(
      tree([
        {
          key: 'runtime',
          type: 'select',
          name: 'Runtime',
          writable: true,
          values: [
            { value: 'podman', label: 'podman' },
            { value: 'docker', label: 'docker' },
          ],
        },
      ]),
    );

    const select = await waitFor(() =>
      screen.getByTestId('plugin-option-runtime').querySelector('select')!,
    );
    // Declared order preserved, plus the empty choice that lets an option be
    // left unset and makes a stale stored value visibly not one of them.
    expect([...select.options].map((o) => o.value)).toEqual(['', 'podman', 'docker']);
  });

  it('runs a button through execute rather than set', async () => {
    mount(tree([{ key: 'cleanup', type: 'execute', name: 'Remove orphans' }]));

    const button = await waitFor(() =>
      screen.getByTestId('plugin-option-cleanup').querySelector('button')!,
    );
    fireEvent.click(button);

    await waitFor(() => expect(calls).toContainEqual({ action: 'execute', path: ['cleanup'] }));
    expect(calls.some((call) => call.action === 'set')).toBe(false);
  });

  // `writable: false` means no `set` is inherited, so the daemon would refuse
  // the write. Offering an edit that cannot succeed is worse than showing none.
  it('renders a read-only option as read-only', async () => {
    stored = 'locked';
    mount(tree([{ key: 'locked', type: 'input', name: 'Locked', writable: false }]));

    const row = await waitFor(() => screen.getByTestId('plugin-option-locked'));
    expect(row.querySelector('input')).toBeDisabled();
    expect(row.textContent).toContain('read-only');
  });

  it('hides a node the daemon marked hidden for this frontend', async () => {
    mount(
      tree([
        { key: 'shown', type: 'input', name: 'Shown', writable: true },
        { key: 'gone', type: 'input', name: 'Gone', hidden: true },
      ]),
    );

    await waitFor(() => expect(screen.getByTestId('plugin-option-shown')).toBeInTheDocument());
    expect(screen.queryByTestId('plugin-option-gone')).toBeNull();
  });

  it('renders nested groups with paths that address the leaf', async () => {
    stored = 'x';
    mount(
      tree([
        {
          key: 'advanced',
          type: 'group',
          name: 'Advanced',
          args: [{ key: 'tweak', type: 'input', name: 'Tweak', writable: true }],
        },
      ]),
    );

    await waitFor(() => screen.getByTestId('plugin-option-advanced-tweak'));
    await waitFor(() =>
      expect(calls).toContainEqual({ action: 'get', path: ['advanced', 'tweak'] }),
    );
  });

  it('says so when a plugin declares no settings', () => {
    mount(tree([]));
    expect(screen.getByText(/declares no settings/)).toBeInTheDocument();
  });
});
