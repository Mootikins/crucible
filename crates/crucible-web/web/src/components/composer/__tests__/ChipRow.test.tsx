import { describe, it, expect, vi, afterEach } from 'vitest';
import { render, screen, fireEvent, cleanup } from '@solidjs/testing-library';
import { Bot } from '@/lib/icons';

vi.mock('@/components/ChatModeControl', () => ({
  ChatModeControl: () => <button type="button" data-testid="chat-mode" aria-label="Mode: Ask" />,
}));

const { ChipRow } = await import('../ChipRow');
type ComposerChip = import('../ChipRow').ComposerChip;

afterEach(cleanup);

/**
 * The one chip row both composers draw under the capsule. It is built from
 * data: a list of key/value entries, each saying how it renders. A surface
 * hands it a list and never places a chip by hand.
 */
describe('ChipRow', () => {
  const onSelect = vi.fn();
  const chips = (): ComposerChip[] => [
    {
      key: 'model',
      label: 'Model',
      value: 'glm-4.7',
      options: [
        { value: 'glm-4.7', label: 'glm-4.7' },
        { value: 'gpt-4o', label: 'gpt-4o' },
      ],
      onSelect,
      testid: 'row-model',
      render: 'select',
    },
    { key: 'mode', label: 'Mode', value: 'ask', render: 'mode' },
    {
      key: 'project',
      label: 'Project',
      value: 'crucible',
      title: '/repos/crucible',
      icon: Bot,
      testid: 'row-project',
      render: 'static',
    },
    {
      key: 'kiln',
      label: 'Kiln',
      value: '',
      defaultLabel: 'docs',
      options: [
        { value: '', label: 'docs', hint: 'default' },
        { value: 'none', label: 'No kiln' },
      ],
      onSelect,
      testid: 'row-kiln',
    },
  ];

  it('renders every entry in order inside one row', () => {
    render(() => <ChipRow chips={chips()} />);
    const row = screen.getByTestId('composer-chip-row');
    const ids = Array.from(row.querySelectorAll('[data-testid]')).map((e) =>
      e.getAttribute('data-testid'),
    );
    expect(ids.indexOf('row-model')).toBeLessThan(ids.indexOf('chat-mode'));
    expect(ids.indexOf('chat-mode')).toBeLessThan(ids.indexOf('row-project'));
    expect(ids.indexOf('row-project')).toBeLessThan(ids.indexOf('row-kiln'));
  });

  it('a select chip opens its options and hands the pick back', () => {
    render(() => <ChipRow chips={chips()} />);
    const chip = screen.getByTestId('row-model');
    expect(chip.tagName).toBe('BUTTON');
    expect(chip.textContent).toContain('glm-4.7');
    fireEvent.click(chip);
    fireEvent.click(screen.getByText('gpt-4o'));
    expect(onSelect).toHaveBeenCalledWith('gpt-4o');
  });

  it('a static chip is a label with the full text in its title, not a control', () => {
    render(() => <ChipRow chips={chips()} />);
    const chip = screen.getByTestId('row-project');
    expect(chip.tagName).not.toBe('BUTTON');
    expect(chip.querySelector('button')).toBeNull();
    expect(chip.textContent).toContain('crucible');
    expect(chip.getAttribute('title')).toBe('/repos/crucible');
    expect(chip.querySelector('svg')).toBeTruthy();
  });

  it('a mode chip renders the round mode control', () => {
    render(() => <ChipRow chips={chips()} />);
    expect(screen.getByTestId('chat-mode').getAttribute('aria-label')).toBe('Mode: Ask');
  });

  it('an empty value reads as its default, marked as such', () => {
    render(() => <ChipRow chips={chips()} />);
    const chip = screen.getByTestId('row-kiln');
    expect(chip.textContent).toContain('docs');
    expect(chip.textContent).toContain('default');
    expect(chip.textContent).not.toContain('Kiln');
  });

  it('a chosen value drops the default mark', () => {
    const list = chips().map((c) => (c.key === 'kiln' ? { ...c, value: 'none' } : c));
    render(() => <ChipRow chips={list} />);
    const chip = screen.getByTestId('row-kiln');
    expect(chip.textContent).toContain('No kiln');
    expect(chip.textContent).not.toContain('default');
  });

  it('draws nothing for an empty list', () => {
    render(() => <ChipRow chips={[]} />);
    expect(screen.queryByTestId('composer-chip-row')).toBeNull();
  });
});
