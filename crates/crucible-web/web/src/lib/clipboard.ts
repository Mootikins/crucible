/** Copy from a user gesture, including HTTP LAN clients without Clipboard API. */
export async function copyText(text: string): Promise<void> {
  if (navigator.clipboard?.writeText) {
    try { await navigator.clipboard.writeText(text); return; } catch { /* Try the user-gesture fallback. */ }
  }
  const focused = document.activeElement as HTMLElement | null;
  const selection = window.getSelection();
  const ranges = selection ? Array.from({ length: selection.rangeCount }, (_, i) => selection.getRangeAt(i).cloneRange()) : [];
  const input = document.createElement('textarea');
  input.dataset.clipboardFallback = '';
  input.value = text;
  input.readOnly = true;
  input.style.cssText = 'position:fixed;left:-10000px;top:0;font-size:16px';
  document.body.append(input);
  try {
    input.focus();
    input.select();
    if (!document.execCommand?.('copy')) throw new Error('Copy was blocked by the browser. Select the text and copy it manually.');
  } finally {
    input.remove();
    focused?.focus({ preventScroll: true });
    if (selection) {
      selection.removeAllRanges();
      for (const range of ranges) selection.addRange(range);
    }
  }
}
