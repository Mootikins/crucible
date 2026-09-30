import { readFileSync } from 'node:fs';
import { resolve as resolvePath } from 'node:path';
import { expect, it } from 'vitest';
import type { components, paths } from '../api-schema';

// The type level half of the gate. `paths` must carry the route, the method and
// the reply shape, so `bunx tsc --noEmit` fails when the generator did not run
// after a handler changed. `just lint types` is what reports that failure.
type PendingInteractionsBody =
  paths['/api/interactions/pending']['get']['responses']['200']['content']['application/json'];
const declaredReply: PendingInteractionsBody = { pending: [] };

// The chat SSE union must narrow on `SessionEventPayload`'s `event` tag, or
// every consumer falls back to a cast and the document buys the browser
// nothing. Adjacent tagging (`#[serde(tag = "event", content = "data")]`)
// carries no `discriminator` keyword either, so these assertions prove
// `openapi-typescript` still emits a tagged union from a plain `oneOf` of
// literal `event` enums.
type SessionEventPayload = components['schemas']['SessionEventPayload'];
type IsNever<T> = [T] extends [never] ? true : false;

// A member that lost its tag would widen the tag to `string` and stop narrowing.
const everyEventTagIsALiteral: string extends SessionEventPayload['event'] ? false : true = true;

type TextDelta = Extract<SessionEventPayload, { event: 'text_delta' }>;
const textDelta: TextDelta = { event: 'text_delta', data: { content: 'hello' } };

type TitleChanged = Extract<SessionEventPayload, { event: 'title_changed' }>;
const titleChangedNarrows: IsNever<TitleChanged> = false;

type InteractionRequested = Extract<SessionEventPayload, { event: 'interaction_requested' }>;
const interactionRequested: InteractionRequested = {
  event: 'interaction_requested',
  data: { request_id: 'i/1', request: { kind: 'ask', question: 'q?' } },
};

// `TranscriptFrame` carries its own tag, `type`, because it is not one of
// `SessionEventPayload`'s variants — the second SSE frame a live event sends
// when it changed the transcript.
type TranscriptFrame = components['schemas']['TranscriptFrame'];
const transcriptFrame: TranscriptFrame = { type: 'transcript', ops: [] };

// The run time half. The committed document is the generator's only input, so a
// test that reads it proves the two committed files describe one route.
const specPath = resolvePath(process.cwd(), '../openapi.json');
const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
  paths: Record<string, Record<string, { responses: Record<string, unknown> }>>;
  components: { schemas: Record<string, { oneOf?: unknown[] } & Record<string, unknown>> };
};

/** The eight `SessionEventPayload` groups — see `crucible-core/src/protocol/session_events/mod.rs`. */
const SESSION_EVENT_GROUPS = [
  'TurnPayload',
  'SetupPayload',
  'SettingsPayload',
  'JobPayload',
  'ReviewPayload',
  'NotificationPayload',
  'WorkflowPayload',
  'SystemPayload',
] as const;

it('the generated schema names the pending-interactions route', () => {
  expect(declaredReply.pending).toEqual([]);

  // A type only import disappears at run time, so this tier proves nothing about
  // the generated file unless it reads the file. Run `just web-contract` to write it.
  const generated = readFileSync(resolvePath(process.cwd(), 'src/lib/api-schema.d.ts'), 'utf8');
  expect(generated).toContain('"/api/interactions/pending"');

  expect(Object.keys(spec.paths)).toContain('/api/interactions/pending');
  const operation = spec.paths['/api/interactions/pending'].get;
  expect(operation).toBeDefined();
  expect(operation.responses['200']).toMatchObject({
    content: {
      'application/json': { schema: { $ref: '#/components/schemas/PendingInteractionsResponse' } },
    },
  });
  expect(spec.components.schemas.PendingInteractionsResponse).toMatchObject({
    type: 'object',
    required: ['pending'],
  });
});

it('the generated session-event union carries every tag the document declares', () => {
  expect(everyEventTagIsALiteral).toBe(true);
  expect(textDelta.data.content).toBe('hello');
  expect(titleChangedNarrows).toBe(false);
  expect(interactionRequested.data.request_id).toBe('i/1');
  expect(transcriptFrame.type).toBe('transcript');

  // Read the names from the document rather than repeating them here. A list in
  // TypeScript is the duplicate the Rust enum exists to remove. Names carry a
  // colon (`note:created`) or a dot (`workflow.step_started`), so the emitted
  // pattern is wider than a plain identifier.
  const generated = readFileSync(resolvePath(process.cwd(), 'src/lib/api-schema.d.ts'), 'utf8');
  const emitted = new Set(
    Array.from(generated.matchAll(/event: "([a-z0-9_.:]+)"/g), (m) => m[1]),
  );

  let total = 0;
  for (const group of SESSION_EVENT_GROUPS) {
    const members = spec.components.schemas[group].oneOf ?? [];
    total += members.length;
    for (const member of members) {
      const tag = eventTagOf(member);
      expect(emitted, `the generated union omits ${tag} (${group})`).toContain(tag);
    }
  }
  // Pins the whole vocabulary's size, so a group that silently stopped
  // deriving `ToSchema` fails here instead of passing with fewer names.
  expect(total).toBe(72);
});

// A variant is either a plain object schema or, when it flattens a `Value`, an
// `allOf` whose last member holds the tag. Adjacent tagging keys the tag as
// `event`, not `type`.
function eventTagOf(member: unknown): string {
  const parts = (member as { allOf?: unknown[] }).allOf ?? [member];
  for (const part of parts) {
    const tag = (part as { properties?: { event?: { enum?: string[] } } }).properties?.event?.enum;
    if (tag) return tag[0];
  }
  throw new Error(`no tag in ${JSON.stringify(member)}`);
}
