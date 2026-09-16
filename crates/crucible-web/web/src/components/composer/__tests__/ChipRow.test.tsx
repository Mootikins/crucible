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

/**
 * The row is ONE line. Chips keep their full text and their full width, so
 * what changes with the width is how MANY of them the row shows: the rest
 * fold into a round `+N` button at the right end, which opens them as a
 * list. Priority decides which chips survive the fold.
 *
 * jsdom has no layout, so the measurement is injected: the test states the
 * row's width and each chip's width and asserts the split.
 */
describe('ChipRow — the row folds into a +N button', () => {
  const onSelect = vi.fn();
  const four = (): ComposerChip[] => [
    { key: 'a', label: 'A', value: 'alpha', priority: 10, testid: 'row-a', render: 'static' },
    { key: 'b', label: 'B', value: 'bravo', priority: 20, testid: 'row-b', render: 'static' },
    {
      key: 'c',
      label: 'C',
      value: 'charlie',
      priority: 30,
      testid: 'row-c',
      options: [
        { value: 'charlie', label: 'charlie' },
        { value: 'delta', label: 'delta' },
      ],
      onSelect,
    },
    { key: 'd', label: 'D', value: 'dingo', priority: 40, testid: 'row-d', render: 'static' },
  ];

  /** "the row is `row`px and the chips are …" */
  const at = (row: number, widths: number[], chips = four()) =>
    render(() => <ChipRow chips={chips} measure={() => ({ row, chips: widths })} />);

  const rowIds = () =>
    Array.from(screen.getByTestId('composer-chip-row').querySelectorAll('[data-testid]')).map((e) =>
      e.getAttribute('data-testid'),
    );

  it('orders the chips by priority, not by the order the surface listed them', () => {
    const shuffled = [
      { key: 'd', label: 'D', value: 'dingo', priority: 40, testid: 'row-d', render: 'static' },
      { key: 'b', label: 'B', value: 'bravo', priority: 20, testid: 'row-b', render: 'static' },
      { key: 'a', label: 'A', value: 'alpha', priority: 10, testid: 'row-a', render: 'static' },
    ] satisfies ComposerChip[];
    at(1000, [40, 40, 40], shuffled);
    expect(rowIds()).toEqual(['row-a', 'row-b', 'row-d']);
  });

  it('shows every chip and no button while they all fit', () => {
    at(1000, [120, 90, 100, 80]);
    expect(rowIds()).toEqual(['row-a', 'row-b', 'row-c', 'row-d']);
    expect(screen.queryByTestId('composer-chip-overflow')).toBeNull();
  });

  it('folds the chips that do not fit into a +N button, counting them', () => {
    // 120 + 4 + 90 = 214 fits in 300; the third chip would make it 318.
    at(300, [120, 90, 100, 80]);
    expect(rowIds()).toEqual(['row-a', 'row-b', 'composer-chip-overflow']);
    const button = screen.getByTestId('composer-chip-overflow');
    expect(button.textContent).toBe('+2');
    expect(button.getAttribute('aria-label')).toBe('2 more');
  });

  it('gives a chip back to the fold when the button itself does not fit', () => {
    // Two chips exactly fill 214px, leaving nothing for the 28px button.
    at(214, [120, 90, 100, 80]);
    expect(rowIds()).toEqual(['row-a', 'composer-chip-overflow']);
    expect(screen.getByTestId('composer-chip-overflow').textContent).toBe('+3');
  });

  it('a folded chip keeps its testid, inside the popover', () => {
    at(300, [120, 90, 100, 80]);
    expect(screen.queryByTestId('row-c')).toBeNull();
    expect(screen.queryByTestId('row-d')).toBeNull();
    fireEvent.click(screen.getByTestId('composer-chip-overflow'));
    const panel = screen.getByTestId('composer-chip-overflow-popout');
    expect(panel.querySelector('[data-testid="row-c"]')).toBeTruthy();
    expect(panel.querySelector('[data-testid="row-d"]')).toBeTruthy();
    // The axis is named in the list, where there is room for it.
    expect(panel.textContent).toContain('C');
    expect(panel.textContent).toContain('dingo');
  });

  it('a folded select chip still opens its options and hands the pick back', () => {
    at(300, [120, 90, 100, 80]);
    fireEvent.click(screen.getByTestId('composer-chip-overflow'));
    fireEvent.click(screen.getByTestId('row-c'));
    fireEvent.click(screen.getByText('delta'));
    expect(onSelect).toHaveBeenCalledWith('delta');
  });

  it('opens on hover as well as on click, and closes on Escape', () => {
    at(300, [120, 90, 100, 80]);
    const button = screen.getByTestId('composer-chip-overflow');
    fireEvent.mouseEnter(button);
    expect(screen.getByTestId('composer-chip-overflow-popout')).toBeInTheDocument();
    fireEvent.keyDown(document, { key: 'Escape' });
    expect(screen.queryByTestId('composer-chip-overflow-popout')).toBeNull();

    fireEvent.click(button);
    expect(screen.getByTestId('composer-chip-overflow-popout')).toBeInTheDocument();
  });

  // A real pointer fires `mouseenter` BEFORE `click`, so the hover has
  // already opened the list by the time the click lands. A toggle would shut
  // it in the same frame it appeared — the list never showed at all.
  it('a click that follows the hover keeps the list open, and the next one shuts it', () => {
    at(300, [120, 90, 100, 80]);
    const button = screen.getByTestId('composer-chip-overflow');
    fireEvent.mouseEnter(button);
    fireEvent.click(button);
    expect(screen.getByTestId('composer-chip-overflow-popout')).toBeInTheDocument();
    fireEvent.click(button);
    expect(screen.queryByTestId('composer-chip-overflow-popout')).toBeNull();
  });

  // The live session's status chips draw nothing until a plugin or a review
  // gives them something to say. A chip with no width is not on the row, so
  // it must not be counted into the fold or listed as a blank row inside it.
  it('does not count a chip that draws nothing', () => {
    at(300, [120, 90, 100, 0]);
    const button = screen.getByTestId('composer-chip-overflow');
    expect(button.textContent).toBe('+1');
    fireEvent.click(button);
    const panel = screen.getByTestId('composer-chip-overflow-popout');
    expect(panel.querySelector('[data-testid="row-c"]')).toBeTruthy();
    expect(panel.textContent).not.toContain('D');
  });

  it('never truncates or shrinks a chip — only the count on the row changes', () => {
    at(300, [120, 90, 100, 80]);
    const row = screen.getByTestId('composer-chip-row');
    for (const el of Array.from(row.querySelectorAll('*'))) {
      const cls = el.getAttribute('class') ?? '';
      expect(cls, `${el.getAttribute('data-testid') ?? el.tagName} truncates`).not.toContain(
        'truncate',
      );
      expect(cls, `${el.getAttribute('data-testid') ?? el.tagName} is capped`).not.toContain(
        'max-w-',
      );
    }
    // One line, and no chip gives up width to a neighbour.
    expect(row.getAttribute('class')).toContain('flex-nowrap');
    for (const el of Array.from(row.children)) {
      expect(el.getAttribute('class') ?? '').toContain('shrink-0');
    }
  });
});

/**
 * The DEFAULT measurement, with no `measure` prop: the row reads its own
 * width and its chips' widths off the document.
 *
 * jsdom reports no geometry, so this stubs the two properties the row reads
 * and lets the real code path run. It is the path that broke in the browser
 * and in no test: `remeasure` runs from an effect as often as from the
 * resize observer, and Solid holds a signal written inside an effect until
 * the update cycle ends — so the pass that draws every chip to measure it
 * read widths of zero, every chip counted as absent, the fold emptied, and
 * the button vanished from under the pointer.
 */
describe('ChipRow — measuring the real row', () => {
  const ROW = 250;
  const CHIP = 100;
  let offset: PropertyDescriptor | undefined;
  let client: PropertyDescriptor | undefined;

  const stubLayout = () => {
    offset = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'offsetWidth');
    client = Object.getOwnPropertyDescriptor(HTMLElement.prototype, 'clientWidth');
    Object.defineProperty(HTMLElement.prototype, 'offsetWidth', {
      configurable: true,
      get() {
        return CHIP;
      },
    });
    Object.defineProperty(HTMLElement.prototype, 'clientWidth', {
      configurable: true,
      get(this: HTMLElement) {
        return this.getAttribute('data-testid') === 'composer-chip-row' ? ROW : 0;
      },
    });
  };

  const restore = () => {
    if (offset) Object.defineProperty(HTMLElement.prototype, 'offsetWidth', offset);
    if (client) Object.defineProperty(HTMLElement.prototype, 'clientWidth', client);
  };

  afterEach(restore);

  const four = (): ComposerChip[] =>
    ['a', 'b', 'c', 'd'].map((k, i) => ({
      key: k,
      label: k.toUpperCase(),
      value: k,
      priority: (i + 1) * 10,
      testid: `real-${k}`,
      render: 'static' as const,
    }));

  it('folds on the widths it reads off the document', async () => {
    stubLayout();
    render(() => <ChipRow chips={four()} />);
    // The first pass draws every chip to measure it and settles a microtask
    // later; nothing here waits on a frame.
    await Promise.resolve();
    await Promise.resolve();

    const row = screen.getByTestId('composer-chip-row');
    const ids = Array.from(row.querySelectorAll('[data-testid]')).map((e) =>
      e.getAttribute('data-testid'),
    );
    // 100 + 4 + 100 = 204 fits in 250, and the 28px button still has room.
    expect(ids).toEqual(['real-a', 'real-b', 'composer-chip-overflow']);
    expect(screen.getByTestId('composer-chip-overflow').textContent).toBe('+2');
  });
});
