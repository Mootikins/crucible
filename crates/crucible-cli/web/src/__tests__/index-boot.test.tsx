import { describe, it, expect, vi } from 'vitest';

// Spy on initializeHighlighter before importing index.tsx
const initSpy = vi.fn().mockResolvedValue(undefined);
vi.mock('@/lib/shiki', async (importOriginal) => {
  const mod = await importOriginal<typeof import('@/lib/shiki')>();
  return { ...mod, initializeHighlighter: initSpy };
});

// Mock App to a no-op component so render() doesn't pull in the entire app
// (providers, contexts, network calls) — we only care that initializeHighlighter
// fires during boot, before render().
vi.mock('../App', () => ({
  default: () => null,
}));

describe('app boot', () => {
  it('calls initializeHighlighter during app startup', async () => {
    // Set up a root element for render() to attach to
    const root = document.createElement('div');
    root.id = 'root';
    document.body.appendChild(root);

    // Importing index.tsx triggers its top-level render()
    await import('../index');

    expect(initSpy).toHaveBeenCalled();
  });
});
