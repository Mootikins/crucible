/**
 * LaTeX math for markdown-it, rendered with KaTeX.
 *
 *   $inline$        → inline math
 *   $$display$$     → block math (own line or spanning lines)
 *
 * The tokenizer is the well-worn markdown-it-katex approach (delimiter
 * validity checks so `$5 and $6` or an escaped `\$` are NOT treated as math).
 * KaTeX renders SYNCHRONOUSLY with `output: 'html'` — plain styled `<span>`s,
 * no MathML — so the result survives DOMPurify untouched (spans + class +
 * inline styles, which the sanitizer already allows) and needs no async pass.
 * `throwOnError: false` renders malformed math as a red error string in place
 * rather than blowing up the whole document render.
 *
 * Requires the KaTeX stylesheet (imported globally in index.tsx); the fonts it
 * references are bundled by Vite from the CSS `url()`s.
 */
import type MarkdownIt from 'markdown-it';
import type StateInline from 'markdown-it/lib/rules_inline/state_inline.mjs';
import type StateBlock from 'markdown-it/lib/rules_block/state_block.mjs';
import katex from 'katex';

/** Whether the `$` at `pos` can open/close math — guards against currency
 * (`$5`), trailing digits, and delimiters hugging whitespace on the wrong side. */
function isValidDelim(state: StateInline, pos: number): { canOpen: boolean; canClose: boolean } {
  const prevChar = pos > 0 ? state.src.charCodeAt(pos - 1) : -1;
  const nextChar = pos + 1 <= state.posMax ? state.src.charCodeAt(pos + 1) : -1;
  let canOpen = true;
  let canClose = true;
  // 0x20 space, 0x09 tab, 0x30–0x39 digits.
  if (prevChar === 0x20 || prevChar === 0x09 || (nextChar >= 0x30 && nextChar <= 0x39)) {
    canClose = false;
  }
  if (nextChar === 0x20 || nextChar === 0x09) {
    canOpen = false;
  }
  return { canOpen, canClose };
}

function mathInline(state: StateInline, silent: boolean): boolean {
  if (state.src[state.pos] !== '$') return false;

  let res = isValidDelim(state, state.pos);
  if (!res.canOpen) {
    if (!silent) state.pending += '$';
    state.pos += 1;
    return true;
  }

  // Scan for the closing `$`, skipping escaped `\$`.
  const start = state.pos + 1;
  let match = start;
  let pos: number;
  while ((match = state.src.indexOf('$', match)) !== -1) {
    pos = match - 1;
    let backslashes = 0;
    while (state.src[pos] === '\\') {
      pos -= 1;
      backslashes += 1;
    }
    if (backslashes % 2 === 0) break; // unescaped closer
    match += 1;
  }

  // No closer, or an empty `$$` — not inline math.
  if (match === -1) {
    if (!silent) state.pending += '$';
    state.pos = start;
    return true;
  }
  if (match - start === 0) {
    if (!silent) state.pending += '$$';
    state.pos = start + 1;
    return true;
  }

  res = isValidDelim(state, match);
  if (!res.canClose) {
    if (!silent) state.pending += '$';
    state.pos = start;
    return true;
  }

  if (!silent) {
    const token = state.push('math_inline', 'math', 0);
    token.markup = '$';
    token.content = state.src.slice(start, match);
  }
  state.pos = match + 1;
  return true;
}

function mathBlock(state: StateBlock, start: number, end: number, silent: boolean): boolean {
  let pos = state.bMarks[start] + state.tShift[start];
  let max = state.eMarks[start];
  if (pos + 2 > max) return false;
  if (state.src.slice(pos, pos + 2) !== '$$') return false;

  pos += 2;
  let firstLine = state.src.slice(pos, max);
  if (silent) return true;

  let found = false;
  let lastLine = '';
  if (firstLine.trim().slice(-2) === '$$') {
    firstLine = firstLine.trim().slice(0, -2);
    found = true;
  }

  let next = start;
  while (!found) {
    next += 1;
    if (next >= end) break;
    pos = state.bMarks[next] + state.tShift[next];
    max = state.eMarks[next];
    if (pos < max && state.tShift[next] < state.blkIndent) break;
    if (state.src.slice(pos, max).trim().slice(-2) === '$$') {
      const lastPos = state.src.slice(0, max).lastIndexOf('$$');
      lastLine = state.src.slice(pos, lastPos);
      found = true;
    }
  }

  state.line = next + 1;
  const token = state.push('math_block', 'math', 0);
  token.block = true;
  token.content =
    (firstLine && firstLine.trim() ? `${firstLine}\n` : '') +
    state.getLines(start + 1, next, state.tShift[start], true) +
    (lastLine && lastLine.trim() ? lastLine : '');
  token.map = [start, state.line];
  token.markup = '$$';
  return true;
}

/** Render a math string to KaTeX HTML; never throws (renders errors in place). */
export function renderMath(src: string, displayMode: boolean): string {
  return katex.renderToString(src, {
    displayMode,
    throwOnError: false,
    output: 'html',
    strict: false,
  });
}

/** markdown-it plugin: `$…$` / `$$…$$` → KaTeX HTML. */
export function mathPlugin(md: MarkdownIt): void {
  md.inline.ruler.after('escape', 'math_inline', mathInline);
  md.block.ruler.after('blockquote', 'math_block', mathBlock, {
    alt: ['paragraph', 'reference', 'blockquote', 'list'],
  });
  md.renderer.rules.math_inline = (tokens, idx) => renderMath(tokens[idx].content, false);
  md.renderer.rules.math_block = (tokens, idx) => `${renderMath(tokens[idx].content, true)}\n`;
}
