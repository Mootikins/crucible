import { readFileSync } from 'node:fs';
import { resolve as resolvePath } from 'node:path';
import { expect, it } from 'vitest';
import type { components, paths } from '../api-schema';

// The type level half of the gate. `paths` must carry the route, the method and
// the reply shape, so `bunx tsc --noEmit` fails when the generator did not run
// after a handler changed. `just lint types` is what reports that failure.
type ModelsBody = paths['/api/models']['get']['responses']['200']['content']['application/json'];
const declaredReply: ModelsBody = { models: ['claude-opus-5'] };

// The SSE union must narrow on its `type` tag, or every consumer falls back to a
// cast and the document buys the browser nothing. The Rust enum carries no
// `discriminator` keyword, so these assertions prove `openapi-typescript` still
// emits a tagged union from a plain `oneOf` of literal `type` enums.
type ChatEvent = components['schemas']['ChatEvent'];
type IsNever<T> = [T] extends [never] ? true : false;

// A member that lost its tag would widen the tag to `string` and stop narrowing.
const everyTagIsALiteral: string extends ChatEvent['type'] ? false : true = true;

type Token = Extract<ChatEvent, { type: 'token' }>;
const token: Token = { type: 'token', content: 'hello' };

type TitleChanged = Extract<ChatEvent, { type: 'title_changed' }>;
const titleChangedNarrows: IsNever<TitleChanged> = false;

// `interaction_requested` flattens a `serde_json::Value`, so the document writes
// it as an `allOf` whose first member is the empty schema. That reaches
// TypeScript as `unknown & { id } & { type }`, which still narrows.
type InteractionRequested = Extract<ChatEvent, { type: 'interaction_requested' }>;
const interactionRequested: InteractionRequested = { type: 'interaction_requested', id: 'i/1' };

// The run time half. The committed document is the generator's only input, so a
// test that reads it proves the two committed files describe one route.
const specPath = resolvePath(process.cwd(), '../openapi.json');
const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
  paths: Record<string, Record<string, { responses: Record<string, unknown> }>>;
  components: { schemas: Record<string, { oneOf?: unknown[] } & Record<string, unknown>> };
};

it('the generated schema names the models route', () => {
  expect(declaredReply.models).toEqual(['claude-opus-5']);

  // A type only import disappears at run time, so this tier proves nothing about
  // the generated file unless it reads the file. Run `just web-contract` to write it.
  const generated = readFileSync(resolvePath(process.cwd(), 'src/lib/api-schema.d.ts'), 'utf8');
  expect(generated).toContain('"/api/models"');

  expect(Object.keys(spec.paths)).toContain('/api/models');
  const operation = spec.paths['/api/models'].get;
  expect(operation).toBeDefined();
  expect(operation.responses['200']).toMatchObject({
    content: { 'application/json': { schema: { $ref: '#/components/schemas/ModelsResponse' } } },
  });
  expect(spec.components.schemas.ModelsResponse).toMatchObject({
    type: 'object',
    required: ['models'],
    properties: { models: { type: 'array', items: { type: 'string' } } },
  });
});

it('the generated chat event union carries every tag the document declares', () => {
  expect(everyTagIsALiteral).toBe(true);
  expect(token.content).toBe('hello');
  expect(titleChangedNarrows).toBe(false);
  expect(interactionRequested.id).toBe('i/1');

  // Read the names from the document rather than repeating them here. A list in
  // TypeScript is the duplicate the Rust enum exists to remove.
  const members = spec.components.schemas.ChatEvent.oneOf ?? [];
  expect(members).toHaveLength(21);

  const generated = readFileSync(resolvePath(process.cwd(), 'src/lib/api-schema.d.ts'), 'utf8');
  const emitted = new Set(Array.from(generated.matchAll(/type: "([a-z_]+)"/g), (m) => m[1]));
  for (const member of members) {
    const tag = tagOf(member);
    expect(emitted, `the generated union omits ${tag}`).toContain(tag);
  }
});

// A variant is either a plain object schema or, when it flattens a `Value`, an
// `allOf` whose last member holds the tag.
function tagOf(member: unknown): string {
  const parts = (member as { allOf?: unknown[] }).allOf ?? [member];
  for (const part of parts) {
    const tag = (part as { properties?: { type?: { enum?: string[] } } }).properties?.type?.enum;
    if (tag) return tag[0];
  }
  throw new Error(`no tag in ${JSON.stringify(member)}`);
}
