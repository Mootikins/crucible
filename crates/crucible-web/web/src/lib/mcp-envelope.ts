/**
 * MCP tool results arrive wrapped in a `{content: [{type: 'text', text}]}`
 * envelope whose `text` payload is itself usually JSON. Pretty-printing the
 * envelope renders the WRAPPER nicely while the actual result stays one
 * escaped line, so callers unwrap first and pretty-print what's inside.
 *
 * Lives here rather than inside ToolCard so the shipped function is the one
 * under test — a copy in the test file would keep passing while the component
 * drifted.
 */

/**
 * Collapse a strict MCP text envelope to its joined text; return `node`
 * unchanged for anything else.
 *
 * Strict means: `content` is the ONLY key, and every entry carries
 * `{type: 'text', text}`. Looser shapes — a domain object that happens to
 * have content+text, an envelope with extra metadata fields, an array mixing
 * text and non-text blocks — stay unchanged so their real structure stays
 * visible to the user.
 */
export function unwrapMcpEnvelope(node: unknown): unknown {
  if (!node || typeof node !== 'object' || Array.isArray(node)) return node;

  const obj = node as Record<string, unknown>;
  const keys = Object.keys(obj);
  if (keys.length !== 1 || keys[0] !== 'content' || !Array.isArray(obj.content)) return node;

  const textBlocks = obj.content.filter(
    (c): c is { type: string; text: string } =>
      !!c &&
      typeof c === 'object' &&
      (c as { type?: unknown }).type === 'text' &&
      typeof (c as { text?: unknown }).text === 'string',
  );
  if (textBlocks.length === 0 || textBlocks.length !== obj.content.length) return node;

  return textBlocks.map((b) => b.text).join('\n');
}
