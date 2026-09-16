import { readFileSync } from 'node:fs';
import { resolve as resolvePath } from 'node:path';
import { expect, it } from 'vitest';
import type { paths } from '../api-schema';

// The type level half of the gate. `paths` must carry the route, the method and
// the reply shape, so `bunx tsc --noEmit` fails when the generator did not run
// after a handler changed. `just lint types` is what reports that failure.
type ModelsBody = paths['/api/models']['get']['responses']['200']['content']['application/json'];
const declaredReply: ModelsBody = { models: ['claude-opus-5'] };

// The run time half. The committed document is the generator's only input, so a
// test that reads it proves the two committed files describe one route.
const specPath = resolvePath(process.cwd(), '../openapi.json');
const spec = JSON.parse(readFileSync(specPath, 'utf8')) as {
  paths: Record<string, Record<string, { responses: Record<string, unknown> }>>;
  components: { schemas: Record<string, unknown> };
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
