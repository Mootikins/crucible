/**
 * An icon button that copies text to the clipboard. A check replaces the
 * icon for a moment, so the copy shows without a toast. The real app does
 * the same in `Message.tsx` and `AssistantTurn.tsx`.
 */
import { Show, createSignal, type Component } from 'solid-js';
import { Check, Copy } from '@/lib/icons';
import { IconButton } from './IconButton';

export const CopyIconButton: Component<{ text: () => string; label: string }> = (props) => {
  const [copied, setCopied] = createSignal(false);
  const copy = async () => {
    try {
      await navigator.clipboard.writeText(props.text());
      setCopied(true);
      setTimeout(() => setCopied(false), 1200);
    } catch {
      // A page without clipboard access copies nothing, and says nothing.
    }
  };
  return (
    <IconButton label={copied() ? 'Copied' : props.label} onClick={() => void copy()}>
      <Show when={copied()} fallback={<Copy class="mk-i" />}>
        <Check class="mk-i mk-ok" />
      </Show>
    </IconButton>
  );
};
