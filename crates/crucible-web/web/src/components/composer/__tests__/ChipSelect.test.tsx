import { describe, it, expect, vi } from 'vitest';
import { render, screen, fireEvent } from '@solidjs/testing-library';
import { ChipSelect, type ChipOption } from '../ChipSelect';

/**
 * The "Run on" menu's shape: flat entries beside a category that drills in.
 *
 * `Remote Machines` carries no value of its own — it is a doorway. That is the
 * distinction these tests exist to hold: a category must not be selectable, or
 * the caller receives a value naming a group rather than a target.
 */
const runOnOptions = (): ChipOption[] => [
  { value: '', label: 'This PC' },
  { value: 'oci:rust', label: 'Container · rust' },
  {
    value: 'ssh',
    label: 'Remote Machines',
    children: [
      { value: 'ssh:example-k3s', label: 'example-k3s', hint: 'aarch64' },
      { value: 'ssh:build-box', label: 'build-box' },
    ],
  },
];

const openMenu = () => fireEvent.click(screen.getByTestId('run-on'));

const renderChip = (onSelect = vi.fn(), options = runOnOptions()) => {
  render(() => (
    <ChipSelect
      name="Run on"
      testid="run-on"
      options={options}
      value=""
      onSelect={onSelect}
      optionTestidPrefix="run-on-opt"
    />
  ));
  return onSelect;
};

describe('ChipSelect submenus', () => {
  it('does not open a flyout until the category is activated', () => {
    renderChip();
    openMenu();
    expect(screen.getByText('Remote Machines')).toBeTruthy();
    expect(screen.queryByTestId('run-on-flyout')).toBeNull();
    expect(screen.queryByText('example-k3s')).toBeNull();
  });

  it('opens the flyout when the category row is clicked, without selecting it', () => {
    const onSelect = renderChip();
    openMenu();
    fireEvent.click(screen.getByText('Remote Machines'));

    expect(screen.getByTestId('run-on-flyout')).toBeTruthy();
    expect(screen.getByText('example-k3s')).toBeTruthy();
    // The whole point of a doorway row: clicking it must not hand the caller
    // 'ssh', which names no machine.
    expect(onSelect).not.toHaveBeenCalled();
  });

  /**
   * What a real pointer actually does: `mouseenter` then `click`.
   *
   * A synthetic click fires no mouseenter, so a toggle-on-click implementation
   * passes every jsdom test and closes the submenu under a real cursor — which
   * is how this shipped to the browser suite before being caught there.
   */
  it('stays open when the category is hovered and then clicked', () => {
    renderChip();
    openMenu();
    const row = screen.getByTestId('run-on-opt-ssh');
    fireEvent.mouseEnter(row);
    expect(screen.getByTestId('run-on-flyout')).toBeTruthy();

    fireEvent.click(row);
    expect(screen.getByTestId('run-on-flyout')).toBeTruthy();
    expect(screen.getByText('example-k3s')).toBeTruthy();
  });

  it('selects a child and closes the whole menu', () => {
    const onSelect = renderChip();
    openMenu();
    fireEvent.click(screen.getByText('Remote Machines'));
    fireEvent.click(screen.getByText('example-k3s'));

    expect(onSelect).toHaveBeenCalledWith('ssh:example-k3s');
    expect(screen.queryByTestId('run-on-flyout')).toBeNull();
    expect(screen.queryByTestId('run-on-popout')).toBeNull();
  });

  it('a plain row still selects normally with a category present', () => {
    const onSelect = renderChip();
    openMenu();
    fireEvent.click(screen.getByText('Container · rust'));
    expect(onSelect).toHaveBeenCalledWith('oci:rust');
  });

  it('hovering a plain row dismisses a flyout a sibling left open', () => {
    renderChip();
    openMenu();
    fireEvent.click(screen.getByText('Remote Machines'));
    expect(screen.getByTestId('run-on-flyout')).toBeTruthy();

    // Two panels both looking live is the failure — the user reads the flyout
    // as belonging to whichever row is highlighted.
    // The row element, not its inner span: mouseenter does not bubble.
    fireEvent.mouseEnter(screen.getByTestId('run-on-opt-oci:rust'));
    expect(screen.queryByTestId('run-on-flyout')).toBeNull();
  });

  it('shows the selected child on the trigger, not the category', () => {
    render(() => (
      <ChipSelect
        name="Run on"
        testid="run-on"
        options={runOnOptions()}
        value="ssh:build-box"
        onSelect={vi.fn()}
      />
    ));
    // Resolving the label has to look inside children, or a chosen machine
    // reads as unset.
    expect(screen.getByTestId('run-on').textContent).toContain('build-box');
  });

  /**
   * A target behind a closed flyout is unfindable by typing unless the filter
   * reaches into children — and with enough hosts, typing is how it gets found.
   */
  it('finds a child by filter text and offers it under its category', () => {
    const onSelect = renderChip(vi.fn(), [
      ...runOnOptions(),
      { value: 'a', label: 'a' },
      { value: 'b', label: 'b' },
      { value: 'c', label: 'c' },
      { value: 'd', label: 'd' },
      { value: 'e', label: 'e' },
      { value: 'f', label: 'f' },
    ]);
    openMenu();

    fireEvent.input(screen.getByLabelText('Search Run on'), { target: { value: 'example' } });

    const hit = screen.getByText('example-k3s');
    expect(hit).toBeTruthy();
    // Flattened under its parent's name, so the row still says where it lives.
    expect(screen.getByText('Remote Machines')).toBeTruthy();

    fireEvent.click(hit);
    expect(onSelect).toHaveBeenCalledWith('ssh:example-k3s');
  });

  it('escape backs out of the flyout before closing the menu', () => {
    renderChip();
    openMenu();
    fireEvent.click(screen.getByText('Remote Machines'));

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByTestId('run-on-flyout')).toBeNull();
    // Still open: backing out of a submenu opened by mistake must not discard
    // the selection the user came here to make.
    expect(screen.getByTestId('run-on-popout')).toBeTruthy();

    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByTestId('run-on-popout')).toBeNull();
  });
});
