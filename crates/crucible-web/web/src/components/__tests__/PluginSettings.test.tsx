import { describe, it, expect, vi, afterEach, beforeEach } from 'vitest';
import { render, cleanup, waitFor, screen, fireEvent } from '@solidjs/testing-library';
import { PluginSettings } from '../PluginSettings';
import type { PluginOptionNode } from '@/lib/api';

const getMock = vi.fn();
const setMock = vi.fn();
const executeMock = vi.fn();
vi.mock('@/lib/api', () => ({
  getPluginOption: (...args: unknown[]) => getMock(...args),
  setPluginOption: (...args: unknown[]) => setMock(...args),
  executePluginOption: (...args: unknown[]) => executeMock(...args),
}));

const addNotification = vi.fn();
vi.mock('@/stores/notificationStore', () => ({
  notificationActions: { addNotification: (...a: unknown[]) => addNotification(...a) },
}));

beforeEach(() => {
  getMock.mockResolvedValue(null);
  setMock.mockResolvedValue(undefined);
  executeMock.mockResolvedValue(undefined);
});
afterEach(() => {
  cleanup();
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
    getMock.mockResolvedValue('flux');
    mount(
      tree([
        { key: 'zarquon', type: 'input', name: 'Flux capacitor', desc: 'Charge level', writable: true },
      ]),
    );

    const row = await waitFor(() => screen.getByTestId('plugin-option-zarquon'));
    expect(row.textContent).toContain('Flux capacitor');
    expect(row.textContent).toContain('Charge level');
    expect(row.querySelector('input')).toHaveValue('flux');
  });

  // Widget kinds are the plugin's word, not an enum this file owns. One added
  // after this file was written must still render — an invisible setting that
  // is nonetheless in effect is the worst outcome.
  it('renders an unrecognised widget kind as a value rather than nothing', async () => {
    getMock.mockResolvedValue('42');
    mount(tree([{ key: 'novel', type: 'colour-wheel', name: 'Hue', writable: true }]));

    const row = await waitFor(() => screen.getByTestId('plugin-option-novel'));
    expect(row.querySelector('input')).toHaveValue('42');
  });

  it('writes through the plugin setter and re-reads what was actually stored', async () => {
    // The setter normalises: what comes back is not what was typed, and the
    // pane must end up showing the stored value.
    getMock.mockResolvedValueOnce('alpine').mockResolvedValueOnce('docker.io/debian');
    mount(tree([{ key: 'image', type: 'input', name: 'Image', writable: true }]));

    const input = await waitFor(() =>
      screen.getByTestId('plugin-option-image').querySelector('input')!,
    );
    fireEvent.change(input, { target: { value: 'debian' } });

    await waitFor(() => expect(setMock).toHaveBeenCalledWith('oci', ['image'], 'debian'));
    await waitFor(() => expect(input).toHaveValue('docker.io/debian'));
  });

  it('rolls back and reports when the plugin refuses a value', async () => {
    getMock.mockResolvedValue('alpine');
    setMock.mockRejectedValue(new Error('unknown image'));
    mount(tree([{ key: 'image', type: 'input', name: 'Image', writable: true }]));

    const input = await waitFor(() =>
      screen.getByTestId('plugin-option-image').querySelector('input')!,
    );
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
    getMock.mockResolvedValue(false);
    mount(tree([{ key: 'flag', type: 'toggle', name: 'Flag', writable: true }]), onChanged);

    const box = await waitFor(() =>
      screen.getByTestId('plugin-option-flag').querySelector('input')!,
    );
    fireEvent.change(box, { target: { checked: true } });

    // The new state, not merely "something was written" — a toggle that always
    // committed its old value would satisfy the invalidation check alone.
    await waitFor(() => expect(setMock).toHaveBeenCalledWith('oci', ['flag'], true));
    await waitFor(() => expect(onChanged).toHaveBeenCalled());
  });

  it('offers a select only the choices the daemon evaluated', async () => {
    getMock.mockResolvedValue('podman');
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

    await waitFor(() => expect(executeMock).toHaveBeenCalledWith('oci', ['cleanup']));
    expect(setMock).not.toHaveBeenCalled();
  });

  // `writable: false` means no `set` is inherited, so the daemon would refuse
  // the write. Offering an edit that cannot succeed is worse than showing none.
  it('renders a read-only option as read-only', async () => {
    getMock.mockResolvedValue('locked');
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
    getMock.mockResolvedValue('x');
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
    expect(getMock).toHaveBeenCalledWith('oci', ['advanced', 'tweak']);
  });

  it('says so when a plugin declares no settings', () => {
    mount(tree([]));
    expect(screen.getByText(/declares no settings/)).toBeInTheDocument();
  });
});
