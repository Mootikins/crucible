import { describe, it, expect, vi, beforeEach } from 'vitest';
import { render, waitFor, fireEvent } from '@solidjs/testing-library';
import { MarkdownPreview } from '../MarkdownPreview';

beforeEach(() => {
  document.body.innerHTML = '';
});

describe('MarkdownPreview (reading view)', () => {
  it('renders an embedded centered HTML block (badges/demo), sanitized', async () => {
    const { getByTestId } = render(() => (
      <MarkdownPreview content={'<p align="center"><img src="https://x/y.svg" alt="demo" /></p>'} />
    ));
    const preview = getByTestId('markdown-preview');
    await waitFor(() => {
      const centered = preview.querySelector('p[align="center"]');
      expect(centered).not.toBeNull();
      expect(centered!.querySelector('img')).not.toBeNull();
    });
  });

  it('copies a code block via its floating copy button', async () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    Object.assign(navigator, { clipboard: { writeText } });

    const { getByTestId } = render(() => (
      <MarkdownPreview content={'```sh\nnpm install\n```'} />
    ));
    const preview = getByTestId('markdown-preview');
    const btn = await waitFor(() => {
      const b = preview.querySelector<HTMLButtonElement>('.md-codeblock [data-copy]');
      expect(b).not.toBeNull();
      return b!;
    });

    fireEvent.click(btn);
    await waitFor(() => expect(writeText).toHaveBeenCalledTimes(1));
    // The copied text is the code, not the button label.
    expect(writeText.mock.calls[0][0]).toContain('npm install');
    // Brief "Copied" affordance.
    expect(btn.textContent).toBe('Copied');
  });
});

describe('MarkdownPreview — frontmatter Properties card', () => {
  it('renders YAML frontmatter as a card, not body text', async () => {
    const { getByTestId } = render(() => (
      <MarkdownPreview content={'---\ntitle: Hello\ntags:\n  - a\n  - b\n---\n# Body\n'} />
    ));
    const preview = getByTestId('markdown-preview');
    await waitFor(() => {
      const card = preview.querySelector('[data-testid="fm-card"]');
      expect(card).not.toBeNull();
      expect(card!.textContent).toContain('title');
      expect(card!.querySelectorAll('.fm-pill')).toHaveLength(2);
    });
    // The raw delimiters never reach the prose.
    expect(preview.textContent).not.toContain('---');
  });

  it('renders TOML (+++) frontmatter as a card too', async () => {
    const { getByTestId } = render(() => (
      <MarkdownPreview content={'+++\ntitle = "Hello"\ntags = ["a"]\n+++\n# Body\n'} />
    ));
    const preview = getByTestId('markdown-preview');
    await waitFor(() => {
      expect(preview.querySelector('[data-testid="fm-card"]')).not.toBeNull();
    });
    expect(preview.textContent).not.toContain('+++');
    expect(preview.textContent).not.toContain('title = ');
  });

  it('omits the card (old strip behavior) when frontmatter is unparseable', async () => {
    const { getByTestId } = render(() => (
      <MarkdownPreview content={'---\nmeta:\n  nested: true\n---\n# Body\n'} />
    ));
    const preview = getByTestId('markdown-preview');
    await waitFor(() => {
      expect(preview.querySelector('h1')).not.toBeNull();
    });
    expect(preview.querySelector('[data-testid="fm-card"]')).toBeNull();
    expect(preview.textContent).not.toContain('nested');
  });

  /**
   * A task box is rendered disabled, because the same pipeline draws chat
   * messages and hover cards, where there is no file behind it. A view that
   * can write the file enables them.
   */
  it('leaves task boxes disabled when no caller can write the file', async () => {
    const { container } = render(() => (
      <MarkdownPreview content={'- [ ] alpha\n- [x] beta\n'} />
    ));
    await waitFor(() =>
      expect(container.querySelectorAll('input.task-checkbox').length).toBe(2),
    );
    for (const box of container.querySelectorAll<HTMLInputElement>('input.task-checkbox')) {
      expect(box.disabled, 'an enabled box that does nothing is worse than one that cannot be pressed').toBe(true);
    }
  });

  it('enables them and reports the SOURCE line when a caller can write', async () => {
    const toggled: number[] = [];
    const { container } = render(() => (
      <MarkdownPreview
        content={'# Tasks\n\n- [ ] alpha\n- [ ] beta\n'}
        path="/k/Tasks.md"
        onToggleTask={(line) => toggled.push(line)}
      />
    ));
    await waitFor(() =>
      expect(container.querySelectorAll('input.task-checkbox:not([disabled])').length).toBe(2),
    );

    // The SECOND box. Its source line is 3, not its index among the boxes —
    // a box can sit inside a blockquote, a nested list or a callout.
    const boxes = container.querySelectorAll<HTMLInputElement>('input.task-checkbox');
    fireEvent.click(boxes[1]);
    expect(toggled).toEqual([3]);
  });
});
