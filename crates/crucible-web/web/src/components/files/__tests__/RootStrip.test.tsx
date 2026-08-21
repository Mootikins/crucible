import { describe, it, expect, vi } from 'vitest';
import { render, fireEvent } from '@solidjs/testing-library';

vi.mock('@/contexts/ProjectContext', () => ({
  useProjectSafe: () => ({
    currentProject: () => null,
    projects: () => [],
    refreshProjects: async () => {},
  }),
}));

import { RootStrip } from '../RootStrip';
import type { SessionRoot } from '@/lib/session-roots';

const WORKSPACE: SessionRoot = {
  kind: 'project',
  path: '/home/me/crucible',
  name: 'crucible',
  origin: 'workspace',
};
const ATTACHED: SessionRoot = {
  kind: 'kiln',
  path: '/home/me/docs',
  name: 'docs',
  origin: 'attached-kiln',
};
const BROWSED: SessionRoot = {
  kind: 'kiln',
  path: '/home/me/archive',
  name: 'archive',
  origin: 'other-kiln',
};

function mount(over: Partial<Parameters<typeof RootStrip>[0]> = {}) {
  const onSelect = vi.fn();
  const onAttach = vi.fn();
  const result = render(() => (
    <RootStrip
      roots={[WORKSPACE, ATTACHED]}
      active={WORKSPACE}
      onSelect={onSelect}
      onAttach={onAttach}
      groups={[]}
      {...over}
    />
  ));
  return { ...result, onSelect, onAttach };
}

describe('RootStrip', () => {
  it('renders one tab per root and marks the active one', () => {
    const { getByTestId } = mount();
    expect(getByTestId('root-tab-project:/home/me/crucible').getAttribute('aria-pressed')).toBe(
      'true',
    );
    expect(getByTestId('root-tab-kiln:/home/me/docs').getAttribute('aria-pressed')).toBe('false');
  });

  it('picking a root selects it', () => {
    const { getByTestId, onSelect } = mount();
    fireEvent.click(getByTestId('root-tab-kiln:/home/me/docs'));
    expect(onSelect).toHaveBeenCalledWith(ATTACHED);
  });

  // Browsing is not attaching: the tree shows the kiln, the agent still
  // cannot read or cite it, and the row has to say so.
  it('dims a kiln the session is not attached to', () => {
    const { getByTestId } = mount({ roots: [WORKSPACE, BROWSED], active: BROWSED });
    const tab = getByTestId('root-tab-kiln:/home/me/archive');
    expect(tab.getAttribute('data-origin')).toBe('other-kiln');
    expect(tab.className).toContain('opacity-60');
    expect(tab.getAttribute('title')).toContain('not attached');
  });

  it('offers Attach only on the browsed kiln, and only while it is active', () => {
    const { queryByTestId } = mount({ roots: [WORKSPACE, BROWSED], active: WORKSPACE });
    expect(queryByTestId('root-attach')).toBeNull();

    const active = mount({ roots: [WORKSPACE, BROWSED], active: BROWSED });
    expect(active.queryByTestId('root-attach')).not.toBeNull();
  });

  // The gate that keeps a navigation gesture from widening the agent's
  // corpus: selecting must never imply attaching.
  it('selecting an unattached kiln does not attach it', () => {
    const { getByTestId, onSelect, onAttach } = mount({
      roots: [WORKSPACE, BROWSED],
      active: WORKSPACE,
    });
    fireEvent.click(getByTestId('root-tab-kiln:/home/me/archive'));
    expect(onSelect).toHaveBeenCalledWith(BROWSED);
    expect(onAttach).not.toHaveBeenCalled();
  });

  it('Attach asks for the browsed kiln explicitly', () => {
    const { getByTestId, onAttach } = mount({ roots: [WORKSPACE, BROWSED], active: BROWSED });
    fireEvent.click(getByTestId('root-attach'));
    expect(onAttach).toHaveBeenCalledWith(BROWSED);
  });

  it('keeps the overflow available even with no roots at all', () => {
    const { queryByTestId } = mount({ roots: [], active: null });
    expect(queryByTestId('root-strip')).not.toBeNull();
  });
});
