/**
 * The palette commands of the `Diff` category.
 *
 * The palette does not know which root the file tree shows. Each command
 * therefore sends `openBranchDiff` on the bus, and the Files panel opens the
 * diff of its browsed root.
 */
import type { PaletteCommand } from '@/components/CommandPalette';
import { getBus } from '@/lib/bus';

export function diffCommands(): PaletteCommand[] {
  return [
    {
      id: 'diff-branch',
      label: 'Diff: branch',
      description: 'Compare the commit at HEAD with the default branch.',
      category: 'Diff',
      keywords: ['diff', 'branch', 'compare', 'git', 'review'],
      action: () => getBus().emit('openBranchDiff', { head: 'HEAD' }),
    },
    {
      id: 'diff-working-tree',
      label: 'Diff: working tree',
      description: 'Compare the working tree with the default branch.',
      category: 'Diff',
      keywords: ['diff', 'working', 'tree', 'changes', 'git', 'review'],
      action: () => getBus().emit('openBranchDiff', { head: null }),
    },
  ];
}
