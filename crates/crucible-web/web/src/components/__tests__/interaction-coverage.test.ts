import { describe, it, expect } from 'vitest';
import { readFileSync } from 'node:fs';
import { join } from 'node:path';
import { INTERACTION_KINDS } from '@/lib/types';

/**
 * The browser half of the renderer-coverage guard.
 *
 * Rust's `InteractionRequest::KINDS` is kept complete by an exhaustive match
 * (`crucible-core/src/interaction/types.rs`). This asserts the browser can
 * actually draw each of them — the property that was NOT true before: seven
 * variants existed, the TUI rendered all seven, and the browser rendered three.
 * A request the browser cannot draw leaves the caller parked until its timeout
 * with nothing on screen to explain why.
 *
 * Reading the handler's source rather than mounting it is deliberate. A render
 * test needs a valid request per kind, so it would encode seven request shapes
 * here and drift from the Rust structs; the question this guard asks is only
 * "is there an arm", and that is answerable from the text.
 */
const handlerPath = join(
  __dirname,
  '..',
  'interactions',
  'InteractionHandler.tsx'
);

describe('interaction renderer coverage', () => {
  const source = readFileSync(handlerPath, 'utf8');

  it.each(INTERACTION_KINDS)('renders the %s kind', (kind) => {
    expect(
      source.includes(`props.request.kind === '${kind}'`),
      `InteractionHandler has no arm for '${kind}'. Add a component for it — ` +
        `a kind the browser cannot draw parks its caller until the timeout.`
    ).toBe(true);
  });

  it('has no arm for a kind that does not exist', () => {
    const arms = [...source.matchAll(/props\.request\.kind === '([a-z_]+)'/g)].map(
      (m) => m[1]
    );
    expect(new Set(arms)).toEqual(new Set(INTERACTION_KINDS));
  });
});
