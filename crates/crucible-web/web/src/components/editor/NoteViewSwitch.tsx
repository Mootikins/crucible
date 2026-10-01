import { BookOpen, Code, Pencil } from '@/lib/icons';
import { ToggleGroup, type ToggleOption } from '@/components/ui/IconToggleGroup';

export type EditorMode = 'live' | 'source' | 'reading';
const modes: readonly ToggleOption<EditorMode>[] = [
  { value: 'live', title: 'Live preview', icon: Pencil },
  { value: 'source', title: 'Source', icon: Code },
  { value: 'reading', title: 'Reading view', icon: BookOpen },
];

export function NoteViewSwitch(props: { mode: EditorMode; onChange: (mode: EditorMode) => void }) {
  return <ToggleGroup label="View" value={props.mode} options={modes} onChange={props.onChange} />;
}
