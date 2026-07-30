/**
 * Recursively normalize a parsed JSON tree so `JSON.stringify(..., null, 2)`
 * produces fully-expanded output: every string field that itself parses as
 * JSON is replaced by its decoded form. An optional `unwrapEnvelope`
 * callback (passed by ToolCard so this module has no concept of MCP) is
 * invoked on every node; returning a different value replaces that node.
 *
 * Safety: a single `remainingDepth` budget is threaded through the entire
 * recursion (string re-parse, array walk, object walk, envelope unwrap).
 * Exceeding it leaves the remaining subtree untouched — never throws, never
 * overflows the stack. It is the ONLY bound needed for string re-parsing too:
 * a string that parses to another string always loses its quotes, so each
 * re-parse strictly shrinks the input and consumes one unit of budget.
 *
 * `Object.fromEntries` is used to rebuild objects so a JSON `__proto__` key
 * is preserved as an own property rather than invoking the prototype setter
 * (which would both pollute the output's prototype and silently drop the
 * key from `JSON.stringify`).
 */

type EnvelopeUnwrapper = (node: unknown) => unknown;

const MAX_TOTAL_DEPTH = 50;

function isRecord(v: unknown): v is Record<string, unknown> {
  return typeof v === 'object' && v !== null && !Array.isArray(v);
}

function tryParseJson(raw: string): unknown | undefined {
  try {
    return JSON.parse(raw);
  } catch {
    return undefined;
  }
}

export function deepPrettyPrintJson(
  node: unknown,
  unwrapEnvelope: EnvelopeUnwrapper = (n) => n,
  remainingDepth: number = MAX_TOTAL_DEPTH,
): unknown {
  if (remainingDepth <= 0) return node;

  const unwrapped = unwrapEnvelope(node);

  if (typeof unwrapped === 'string') {
    const next = tryParseJson(unwrapped);
    if (next === undefined) return unwrapped;
    // Recurse rather than loop: `next` may itself be a string carrying yet
    // more encoded JSON, and the recursive call handles that case. Each
    // re-parse consumes one unit of budget, so a JSON-in-JSON-in-… chain
    // stops at the cap instead of resetting per level.
    return deepPrettyPrintJson(next, unwrapEnvelope, remainingDepth - 1);
  }

  if (Array.isArray(unwrapped)) {
    return unwrapped.map((child) => deepPrettyPrintJson(child, unwrapEnvelope, remainingDepth - 1));
  }

  if (isRecord(unwrapped)) {
    // Object.fromEntries preserves `__proto__` as an ordinary key. A plain
    // `{}` target + bracket assignment would invoke the prototype setter,
    // dropping the field and polluting the output's prototype.
    return Object.fromEntries(
      Object.entries(unwrapped).map(([k, v]) => [
        k,
        deepPrettyPrintJson(v, unwrapEnvelope, remainingDepth - 1),
      ]),
    );
  }

  return unwrapped;
}

/** Current total-depth budget (exposed for tests). */
export const DEPTH_BUDGET = MAX_TOTAL_DEPTH;
