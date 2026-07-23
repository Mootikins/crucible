/**
 * Display name for a kiln. The home kiln lives at `~/.crucible` with no
 * registered name, so every basename fallback in the UI was labeling it
 * ".crucible" — a data-dir dotfile, not a name. ONE helper owns the
 * fallback chain: registered name → basename → "Home kiln" for any
 * `.crucible` data dir (or an empty path).
 */
export function kilnLabel(path: string, name?: string | null): string {
  const trimmed = name?.trim();
  if (trimmed) return trimmed;
  const base = path.replace(/\/+$/, '').split('/').pop() ?? '';
  if (!base || base === '.crucible') return 'Home kiln';
  return base;
}
