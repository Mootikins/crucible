/**
 * The two cuts the note view makes in a note's text: the frontmatter from the
 * body, and the body at each inline `:::hunk` block. The real app reads the
 * frontmatter with `lib/frontmatter.ts`, and hunks from the review store.
 */
export type NoteProps = Record<string, string | string[]>;

export function parseFront(src: string): [NoteProps, string] {
  if (!src.startsWith('---\n')) return [{}, src];
  const end = src.indexOf('\n---', 4);
  const props: NoteProps = {};
  for (const line of src.slice(4, end).split('\n')) {
    const m = line.match(/^(\w+):\s*(.*)$/);
    if (m) props[m[1]!] = m[2]!;
  }
  if (typeof props.tags === 'string') props.tags = props.tags.replace(/^\[|\]$/g, '').split(',').map((t) => t.trim()).filter(Boolean);
  return [props, src.slice(end + 4).replace(/^\n/, '')];
}

export type Segment = { kind: 'markdown'; text: string } | { kind: 'hunk'; id: string };

export function segments(body: string): Segment[] {
  const out: Segment[] = [];
  const re = /:::hunk (\w+)\n[\s\S]*?\n:::\n?/g;
  let at = 0;
  for (const m of body.matchAll(re)) {
    out.push({ kind: 'markdown', text: body.slice(at, m.index) });
    out.push({ kind: 'hunk', id: m[1]! });
    at = m.index! + m[0].length;
  }
  out.push({ kind: 'markdown', text: body.slice(at) });
  return out;
}
