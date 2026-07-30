import { describe, it, expect } from 'vitest';
import { deepPrettyPrintJson, DEPTH_BUDGET } from '../pretty-print';
// The unwrapper ToolCard actually ships — importing it (rather than copying
// the shape into this file) is what keeps these envelope cases honest.
import { unwrapMcpEnvelope as MCP_UNWRAP } from '../mcp-envelope';

describe('deepPrettyPrintJson', () => {
  it('leaves primitives untouched', () => {
    expect(deepPrettyPrintJson(42)).toBe(42);
    expect(deepPrettyPrintJson('plain text')).toBe('plain text');
    expect(deepPrettyPrintJson(true)).toBe(true);
    expect(deepPrettyPrintJson(null)).toBe(null);
  });

  it('parses a JSON-carrying string into a structured value', () => {
    const out = deepPrettyPrintJson('{"a":1,"b":[2,3]}');
    expect(out).toEqual({ a: 1, b: [2, 3] });
  });

  it('re-parses double-encoded JSON (JSON inside a JSON string)', () => {
    const double = JSON.stringify(JSON.stringify({ deep: { nested: [1, 2] } }));
    const out = deepPrettyPrintJson(double);
    expect(out).toEqual({ deep: { nested: [1, 2] } });
  });

  it('walks object fields and pretty-prints any string field that is JSON', () => {
    const out = deepPrettyPrintJson({
      plain: 'hello',
      structured: '{"x":1,"y":2}',
      nested: { inner: '[1, 2, 3]' },
    });
    expect(out).toEqual({
      plain: 'hello',
      structured: { x: 1, y: 2 },
      nested: { inner: [1, 2, 3] },
    });
  });

  it('walks arrays and pretty-prints JSON-carrying string elements', () => {
    const out = deepPrettyPrintJson(['just text', '{"a": 1}', ['[1, 2]', 'plain']]);
    expect(out).toEqual(['just text', { a: 1 }, [[1, 2], 'plain']]);
  });

  it('uses the envelope unwinder at every depth', () => {
    const envelope = {
      content: [{ type: 'text', text: '{"result": "ok"}' }],
    };
    const out = deepPrettyPrintJson(envelope, MCP_UNWRAP);
    expect(out).toEqual({ result: 'ok' });
  });

  it('unwraps envelopes nested inside larger responses', () => {
    const nested = {
      items: [
        { content: [{ type: 'text', text: '{"i": 1}' }] },
        { content: [{ type: 'text', text: '{"i": 2}' }] },
      ],
      meta: 'plain',
    };
    const out = deepPrettyPrintJson(nested, MCP_UNWRAP);
    expect(out).toEqual({
      items: [{ i: 1 }, { i: 2 }],
      meta: 'plain',
    });
  });

  it('preserves the top-level shape when nothing parses', () => {
    expect(deepPrettyPrintJson({ a: 'not json', b: 1 })).toEqual({ a: 'not json', b: 1 });
  });

  it('keeps a quoted-string-that-isnt-json as a string', () => {
    expect(deepPrettyPrintJson('"\\nfoo bar"')).toBe('\nfoo bar');
  });

  it('preserves a __proto__ key as an ordinary own property', () => {
    // Bracket-assignment would invoke the prototype setter; Object.fromEntries
    // keeps it as a field. JSON.stringify must still emit it.
    const input = JSON.parse('{"__proto__":{"polluted":true}}');
    const out = deepPrettyPrintJson(input) as Record<string, unknown>;
    expect(Object.prototype.hasOwnProperty.call(out, '__proto__')).toBe(true);
    expect(JSON.stringify(out)).toContain('"__proto__"');
    expect((Object.getPrototypeOf(out) as object).constructor).toBe(Object);
  });

  it('caps total recursion depth to defuse deeply nested DoS payloads', () => {
    // A JSON-carrying string leaf is the discriminator: inside the budget it
    // decodes, past the budget it must be left verbatim. Asserting only
    // "still an object" would pass with no cap at all.
    const nest = (depth: number): unknown => {
      let node: unknown = '{"leaf":true}';
      for (let i = 0; i < depth; i++) node = { nested: node };
      return node;
    };
    const descend = (node: unknown, depth: number): unknown => {
      let cursor = node;
      for (let i = 0; i < depth; i++) cursor = (cursor as { nested: unknown }).nested;
      return cursor;
    };

    const shallow = DEPTH_BUDGET - 5;
    expect(descend(deepPrettyPrintJson(nest(shallow)), shallow)).toEqual({ leaf: true });

    const beyond = DEPTH_BUDGET + 50;
    expect(descend(deepPrettyPrintJson(nest(beyond)), beyond)).toBe('{"leaf":true}');
  });

  it('spends the same depth budget on string re-parsing as on tree walking', () => {
    // The budget is ONE allowance threaded through the whole recursion, so an
    // identical string chain decodes fully at the root and only partially
    // once object nesting has already spent most of the budget. A chain long
    // enough to exhaust the budget on its own isn't constructible —
    // JSON.stringify grows exponentially and V8's string-length cap bites
    // first — so depth is what makes the shared budget observable.
    const chain = (layers: number): string => {
      let s = '"innermost"';
      for (let i = 0; i < layers; i++) s = JSON.stringify(s);
      return s;
    };
    const LAYERS = 8;

    // At the root the full budget is available: the chain decodes completely.
    expect(deepPrettyPrintJson(chain(LAYERS))).toBe('innermost');

    // Buried under nesting that has already consumed all but a few units, the
    // same chain runs out of budget mid-decode and stays partly encoded.
    let buried: unknown = chain(LAYERS);
    const NESTING = DEPTH_BUDGET - 3;
    for (let i = 0; i < NESTING; i++) buried = { nested: buried };

    let cursor = deepPrettyPrintJson(buried);
    for (let i = 0; i < NESTING; i++) cursor = (cursor as { nested: unknown }).nested;
    expect(typeof cursor).toBe('string');
    expect(cursor).not.toBe('innermost');
    expect(cursor as string).toContain('\\');
  });

  it('does not unwrap loose { content: [{ text }] } shapes that lack the MCP discriminator', () => {
    // A domain object that happens to have content+text must NOT be unwrapped.
    const domain = {
      content: [{ text: 'not an envelope' }],
      meta: { important: true },
    };
    const out = deepPrettyPrintJson(domain, MCP_UNWRAP);
    expect(out).toEqual(domain);
  });

  it('preserves sibling fields alongside a strict MCP envelope', () => {
    // An envelope that ALSO carries non-MCP fields keeps the siblings; the
    // unwrapper only fires when content is the ONLY key. The text payload
    // still decodes (deepPrettyPrintJson walks every string), but the outer
    // envelope structure is NOT collapsed into just the joined text.
    const envelope = {
      content: [{ type: 'text', text: '{"ok":true}' }],
      role: 'assistant',
    };
    const out = deepPrettyPrintJson(envelope, MCP_UNWRAP);
    expect(out).toEqual({
      content: [{ type: 'text', text: { ok: true } }],
      role: 'assistant',
    });
  });
});
