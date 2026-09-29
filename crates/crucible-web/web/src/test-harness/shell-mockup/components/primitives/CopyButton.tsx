/** A button that copies text to the clipboard, then says so for a moment. */
import { createSignal, type Component } from 'solid-js';
import { Button } from './Button';

export const CopyButton: Component<{ text: () => string; label: string; copiedLabel?: string }> = (props) => {
  const [copied, setCopied] = createSignal(false);
  const copy = async () => {
    await navigator.clipboard?.writeText(props.text());
    setCopied(true);
    setTimeout(() => setCopied(false), 1400);
  };
  return <Button onClick={() => void copy()}>{copied() ? props.copiedLabel ?? 'Copied' : props.label}</Button>;
};
