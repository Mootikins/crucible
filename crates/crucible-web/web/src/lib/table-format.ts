/**
 * Markdown table source formatter: pads every cell so the pipes line up
 * column-by-column (the live-preview editor shows revealed table source in
 * a monospace font, so alignment is real). Preserves the delimiter row's
 * `:---` / `:---:` / `---:` alignment markers and pads cell text to match
 * (left / center / right).
 */

type Align = 'left' | 'center' | 'right';

interface DelimCol {
  left: boolean;
  right: boolean;
}

const DELIM_CELL_RE = /^\s*(:?)-+(:?)\s*$/;

/** Split a table row's inner text on unescaped pipes (`\|` stays in-cell). */
function splitCells(inner: string): string[] {
  const cells: string[] = [];
  let cur = '';
  for (let i = 0; i < inner.length; i++) {
    const ch = inner[i];
    if (ch === '\\' && inner[i + 1] === '|') {
      cur += '\\|';
      i++;
      continue;
    }
    if (ch === '|') {
      cells.push(cur);
      cur = '';
      continue;
    }
    cur += ch;
  }
  cells.push(cur);
  return cells;
}

interface Row {
  indent: string;
  cells: string[];
  isDelim: boolean;
}

function parseRow(line: string): Row | null {
  const m = /^(\s*)\|(.*)$/.exec(line);
  if (!m) return null;
  let inner = m[2];
  // Trailing unescaped pipe closes the row; strip it (and trailing space).
  const trimmed = inner.replace(/\s+$/, '');
  if (trimmed.endsWith('|') && !trimmed.endsWith('\\|')) {
    inner = trimmed.slice(0, -1);
  }
  const cells = splitCells(inner).map((c) => c.trim());
  const isDelim = cells.length > 0 && cells.every((c) => DELIM_CELL_RE.test(c));
  return { indent: m[1], cells, isDelim };
}

const pad = (text: string, width: number, align: Align): string => {
  const extra = Math.max(0, width - text.length);
  if (align === 'right') return ' '.repeat(extra) + text;
  if (align === 'center') {
    const l = Math.floor(extra / 2);
    return ' '.repeat(l) + text + ' '.repeat(extra - l);
  }
  return text + ' '.repeat(extra);
};

/**
 * Format the lines of one markdown table. Returns null when the lines do not
 * look like a table (no `|` rows). Idempotent: formatting formatted output
 * is a no-op.
 */
export function formatTableLines(lines: string[]): string[] | null {
  const rows = lines.map(parseRow);
  if (rows.some((r) => r === null) || rows.length === 0) return null;
  const parsed = rows as Row[];

  const delims = new Map<number, DelimCol[]>();
  let columns = 0;
  for (const [i, row] of parsed.entries()) {
    columns = Math.max(columns, row.cells.length);
    if (row.isDelim) {
      delims.set(
        i,
        row.cells.map((c) => {
          const m = DELIM_CELL_RE.exec(c)!;
          return { left: m[1] === ':', right: m[2] === ':' };
        }),
      );
    }
  }
  if (columns === 0) return null;

  // Column alignment from the FIRST delimiter row (the header separator).
  const firstDelim = [...delims.values()][0] ?? [];
  const alignOf = (col: number): Align => {
    const d = firstDelim[col];
    if (!d) return 'left';
    if (d.left && d.right) return 'center';
    if (d.right) return 'right';
    return 'left';
  };

  const widths: number[] = Array.from({ length: columns }, () => 3);
  for (const row of parsed) {
    if (row.isDelim) continue;
    row.cells.forEach((c, col) => {
      widths[col] = Math.max(widths[col], c.length);
    });
  }

  const indent = parsed[0].indent;
  return parsed.map((row, i) => {
    const cells: string[] = [];
    for (let col = 0; col < columns; col++) {
      if (row.isDelim) {
        const d = delims.get(i)![col] ?? { left: false, right: false };
        const dashes = widths[col] - (d.left ? 1 : 0) - (d.right ? 1 : 0);
        cells.push(`${d.left ? ':' : ''}${'-'.repeat(Math.max(1, dashes))}${d.right ? ':' : ''}`);
      } else {
        cells.push(pad(row.cells[col] ?? '', widths[col], alignOf(col)));
      }
    }
    return `${indent}| ${cells.join(' | ')} |`;
  });
}
