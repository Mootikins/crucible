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
