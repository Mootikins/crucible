import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor, fireEvent } from '@solidjs/testing-library';
import { NewSessionDialog } from '../NewSessionDialog';

vi.mock('@/lib/api', () => ({
  getConfig: vi.fn(async () => ({ kiln_path: '/vault/docs' })),
  listKilns: vi.fn(async () => [
    { path: '/vault/docs', name: 'docs' },
    { path: '/vault/notes', name: 'notes' },
  ]),
  listProjects: vi.fn(async () => [
    { path: '/code/crucible', name: 'crucible', kilns: [], last_accessed: '' },
  ]),
}));

describe('NewSessionDialog', () => {
  beforeEach(() => vi.clearAllMocks());

  it('prefills the default kiln and lists projects', async () => {
    const { getByTestId } = render(() => (
      <NewSessionDialog open={true} onClose={() => {}} />
    ));
    await waitFor(() => {
      const kiln = getByTestId('new-session-kiln') as HTMLSelectElement;
      expect(kiln.value).toBe('/vault/docs');
      expect(kiln.options.length).toBe(2);
    });
    const ws = getByTestId('new-session-workspace') as HTMLSelectElement;
    // "None" + one project
    expect(ws.options.length).toBe(2);
    expect(ws.value).toBe('');
  });

  it('emits crucible:create-session with the chosen kiln/workspace and closes', async () => {
    const onClose = vi.fn();
    const seen: Array<{ kiln?: string; workspace?: string }> = [];
    const listener = (e: Event) => seen.push((e as CustomEvent).detail);
    window.addEventListener('crucible:create-session', listener);

    const { getByTestId } = render(() => (
      <NewSessionDialog open={true} onClose={onClose} />
    ));
    await waitFor(() => {
      expect((getByTestId('new-session-kiln') as HTMLSelectElement).value).toBe('/vault/docs');
    });

    fireEvent.change(getByTestId('new-session-kiln'), { target: { value: '/vault/notes' } });
    fireEvent.change(getByTestId('new-session-workspace'), { target: { value: '/code/crucible' } });
    fireEvent.click(getByTestId('new-session-create'));

    expect(seen).toEqual([{ kiln: '/vault/notes', workspace: '/code/crucible' }]);
    expect(onClose).toHaveBeenCalled();
    window.removeEventListener('crucible:create-session', listener);
  });

  it('omits workspace when "None" is selected', async () => {
    const seen: Array<{ kiln?: string; workspace?: string }> = [];
    const listener = (e: Event) => seen.push((e as CustomEvent).detail);
    window.addEventListener('crucible:create-session', listener);

    const { getByTestId } = render(() => (
      <NewSessionDialog open={true} onClose={() => {}} />
    ));
    await waitFor(() => {
      expect((getByTestId('new-session-kiln') as HTMLSelectElement).value).toBe('/vault/docs');
    });
    fireEvent.click(getByTestId('new-session-create'));

    expect(seen).toEqual([{ kiln: '/vault/docs', workspace: undefined }]);
    window.removeEventListener('crucible:create-session', listener);
  });
});
