/**
 * The contract sweep's plan: one entry per OpenAPI operation.
 *
 * `crates/crucible-web/openapi.json` carries method, path, parameters and
 * response schema for every route the axum router serves. That is exactly what
 * a live request needs, so the live tier does not need a hand-written spec per
 * route — it needs this table, built from the document at run time.
 *
 * Two consumers, one table:
 *
 *  - `e2e/live/contract.live.spec.ts` turns each entry into a Playwright test.
 *    A route added in Rust regenerates the document, and the next live run has
 *    one more test with no new file.
 *  - `bun run api:specs` prints the table and fails when an operation carries
 *    no entry, so the skip reasons stay readable without booting the tier.
 *
 * Nothing here is generated INTO the tree. "gen" is the plan, not a file.
 */
import { readFileSync } from 'node:fs';
import path from 'node:path';
import { fileURLToPath } from 'node:url';

const HERE = path.dirname(fileURLToPath(import.meta.url));

/** The committed document. Never hand-edited; `just openapi` regenerates it. */
export const DOCUMENT_FILE = path.join(HERE, '..', '..', 'openapi.json');

export interface OpenApiDocument {
  openapi: string;
  paths: Record<string, Record<string, OpenApiOperation | unknown>>;
  components: { schemas: Record<string, unknown> };
}

export interface OpenApiOperation {
  operationId?: string;
  parameters?: OpenApiParameter[];
  requestBody?: unknown;
  responses: Record<string, { content?: Record<string, { schema?: unknown }> }>;
}

export interface OpenApiParameter {
  name: string;
  in: 'path' | 'query' | 'header' | 'cookie';
  required?: boolean;
}

/** The HTTP methods an OpenAPI path item may carry. */
const METHODS = ['get', 'put', 'post', 'delete', 'options', 'head', 'patch', 'trace'] as const;
type Method = (typeof METHODS)[number];

export function loadDocument(file: string = DOCUMENT_FILE): OpenApiDocument {
  return JSON.parse(readFileSync(file, 'utf-8')) as OpenApiDocument;
}

/**
 * Every value a path or query parameter can be filled with.
 *
 * The live tier publishes the kiln in `.live-state.json`; the session is made
 * by the spec's own `beforeAll`, because a session is cheap and the setup that
 * every other lane shares should not grow a field for one sweep.
 */
export interface ProbeValues {
  /** A session the sweep created for itself. */
  sessionId: string;
  /** `state.kilnDir` — the seeded kiln. */
  kilnDir: string;
  /** Absolute path of a file that exists in the kiln. */
  filePath: string;
  /** The name of the note in that file. */
  noteName: string;
  /** Absolute path of a canvas file. It need not exist: 404 is documented. */
  canvasPath: string;
  /** A directory the project routes are asked about. */
  projectPath: string;
  /** A word to search for. */
  searchTerm: string;
  /** A plugin id for the caller header. */
  pluginId: string;
  /** A skill name to ask for. */
  skillName: string;
}

/** What `bun run api:specs` prints the table with. */
export const PLACEHOLDER_VALUES: ProbeValues = {
  sessionId: '<session>',
  kilnDir: '<kiln>',
  filePath: '<kiln>/Seed.md',
  noteName: 'Seed',
  canvasPath: '<kiln>/Probe.canvas',
  projectPath: '<kiln>',
  searchTerm: 'seed',
  pluginId: 'contract-sweep',
  skillName: 'contract-sweep-probe',
};

/** One filled parameter, or the reason it cannot be filled. */
type Filled = { value: string } | { unfillable: string };

/**
 * Where each parameter's value comes from, by operation and parameter name.
 *
 * A parameter with no rule here and no rule in `BY_NAME` is unfillable, and its
 * operation is skipped with that parameter named — a new route with a new
 * parameter therefore announces itself rather than passing silently.
 */
const BY_OPERATION: Record<string, Record<string, (v: ProbeValues) => string>> = {
  'GET /api/backlinks': { note: (v) => v.noteName },
  'GET /api/canvas': { path: (v) => v.canvasPath },
  'GET /api/file/raw': { path: (v) => v.filePath },
  'GET /api/kiln/file': { path: (v) => v.filePath },
  'GET /api/project/get': { path: (v) => v.projectPath },
  'GET /api/notes/resolve': { name: (v) => v.noteName },
  'GET /api/notes/{name}': { name: (v) => v.noteName },
  'GET /api/skills/{name}': { name: (v) => v.skillName },
};

/** Parameters whose meaning does not change with the route. */
const BY_NAME: Record<string, (v: ProbeValues) => string> = {
  id: (v) => v.sessionId,
  session_id: (v) => v.sessionId,
  kiln: (v) => v.kilnDir,
  root: (v) => v.kilnDir,
  q: (v) => v.searchTerm,
  'x-crucible-plugin': (v) => v.pluginId,
};

/**
 * Operations a sweep must not drive, and why.
 *
 * The plan calls this the skip table. Each reason says what the sweep would do
 * to the running daemon, or why the request would never return, and names the
 * live spec that covers the route by hand where one does.
 */
const HAND_COVERED: Record<string, string> = {
  'POST /api/session': 'session-management, session-lifecycle and session-path drive it',
  'PUT /api/session/{id}/title': 'session-management and session-lifecycle drive it',
  'POST /api/chat/send': 'session-lifecycle and session-path drive it',
  'DELETE /api/session/{id}': 'session-lifecycle drives it through the context menu',
  'POST /api/session/{id}/archive': 'session-lifecycle drives it through the hover control',
  'PUT /api/notes/{name}': 'kiln-truth and kiln-restart drive it',
  'PUT /api/kiln/file': 'kiln-truth and conflict drive it, conflict leg included',
};

/** A response that never completes, or never speaks HTTP at all. */
const NON_REQUEST: Record<string, string> = {
  'GET /api/chat/events/{session_id}': 'server-sent event stream: the body never ends',
  'GET /api/fs/events': 'server-sent event stream: the body never ends',
  'GET /api/plugins/events': 'server-sent event stream: the body never ends',
  'GET /api/surfaces/events': 'server-sent event stream: the body never ends',
  'GET /api/terminal/ws': 'websocket upgrade: the document answers 101, not a body',
};

export interface ExercisedOperation {
  kind: 'exercise';
  key: string;
  method: Method;
  path: string;
  operationId?: string;
  /** Path and query, ready to hand to the request fixture. */
  url: string;
  headers: Record<string, string>;
  /** Every status the document lists for this operation. */
  documented: number[];
  /** The `$ref` of the 200 body's schema, when the document names one. */
  okSchemaRef?: string;
  /** The media type the document gives the 200 body. */
  okMediaType?: string;
}

export interface SkippedOperation {
  kind: 'skip';
  key: string;
  method: Method;
  path: string;
  operationId?: string;
  reason: string;
}

export type PlannedOperation = ExercisedOperation | SkippedOperation;

function fill(key: string, param: OpenApiParameter, values: ProbeValues): Filled {
  const byOperation = BY_OPERATION[key]?.[param.name];
  if (byOperation) return { value: byOperation(values) };
  const byName = BY_NAME[param.name];
  if (byName) return { value: byName(values) };
  return { unfillable: param.name };
}

function statuses(op: OpenApiOperation): number[] {
  return Object.keys(op.responses)
    .map((code) => Number(code))
    .filter((code) => Number.isFinite(code))
    .sort((a, b) => a - b);
}

function okBody(op: OpenApiOperation): { ref?: string; mediaType?: string } {
  const content = op.responses['200']?.content;
  if (!content) return {};
  const mediaType = Object.keys(content)[0];
  const schema = content[mediaType]?.schema as { $ref?: string } | undefined;
  return { ref: schema?.$ref, mediaType };
}

/** Build one entry for every operation in the document, in document order. */
export function buildPlan(doc: OpenApiDocument, values: ProbeValues): PlannedOperation[] {
  const plan: PlannedOperation[] = [];
  for (const [routePath, item] of Object.entries(doc.paths)) {
    const shared = ((item as { parameters?: OpenApiParameter[] }).parameters ?? []) as OpenApiParameter[];
    for (const method of METHODS) {
      const op = item[method] as OpenApiOperation | undefined;
      if (!op) continue;
      const key = `${method.toUpperCase()} ${routePath}`;
      const head = { key, method, path: routePath, operationId: op.operationId };

      const nonRequest = NON_REQUEST[key];
      if (nonRequest) {
        plan.push({ kind: 'skip', ...head, reason: nonRequest });
        continue;
      }
      if (method !== 'get') {
        const covered = HAND_COVERED[key];
        plan.push({
          kind: 'skip',
          ...head,
          reason: covered
            ? `${method.toUpperCase()} changes the daemon's records; ${covered}`
            : `${method.toUpperCase()} changes the daemon's records, and no live spec drives it yet`,
        });
        continue;
      }
      if (op.requestBody) {
        plan.push({ kind: 'skip', ...head, reason: 'the operation declares a request body' });
        continue;
      }

      const params = [...shared, ...(op.parameters ?? [])].filter((p) => p.required);
      const query = new URLSearchParams();
      const headers: Record<string, string> = {};
      let url = routePath;
      const unfillable: string[] = [];
      for (const param of params) {
        const filled = fill(key, param, values);
        if ('unfillable' in filled) {
          unfillable.push(filled.unfillable);
          continue;
        }
        if (param.in === 'path') url = url.replace(`{${param.name}}`, encodeURIComponent(filled.value));
        else if (param.in === 'query') query.set(param.name, filled.value);
        else if (param.in === 'header') headers[param.name] = filled.value;
      }
      if (unfillable.length > 0) {
        plan.push({
          kind: 'skip',
          ...head,
          reason: `no probe value for the required parameter${unfillable.length > 1 ? 's' : ''} ${unfillable.join(', ')}`,
        });
        continue;
      }

      const body = okBody(op);
      plan.push({
        kind: 'exercise',
        ...head,
        url: query.size > 0 ? `${url}?${query.toString()}` : url,
        headers,
        documented: statuses(op),
        okSchemaRef: body.ref,
        okMediaType: body.mediaType,
      });
    }
  }
  return plan;
}

/** `METHOD /path` for every operation the document declares, in document order. */
export function operationKeys(doc: OpenApiDocument): string[] {
  const keys: string[] = [];
  for (const [routePath, item] of Object.entries(doc.paths)) {
    for (const method of METHODS) {
      if (item[method]) keys.push(`${method.toUpperCase()} ${routePath}`);
    }
  }
  return keys;
}

/** Count the operations the document declares, however the plan treats them. */
export function countOperations(doc: OpenApiDocument): number {
  return operationKeys(doc).length;
}

function main(): void {
  const doc = loadDocument();
  const plan = buildPlan(doc, PLACEHOLDER_VALUES);
  const exercised = plan.filter((entry) => entry.kind === 'exercise');
  const skipped = plan.filter((entry): entry is SkippedOperation => entry.kind === 'skip');

  console.log(`document: ${DOCUMENT_FILE}`);
  console.log(`paths ${Object.keys(doc.paths).length}, operations ${countOperations(doc)}`);
  console.log(`exercised ${exercised.length}, skipped ${skipped.length}\n`);

  console.log('EXERCISED');
  for (const entry of exercised) {
    const schema = entry.okSchemaRef ? entry.okSchemaRef.split('/').pop() : `(${entry.okMediaType ?? 'no 200 body'})`;
    console.log(`  ${entry.key}\n    ${entry.url}\n    statuses ${entry.documented.join(', ')} | 200 ${schema}`);
  }

  console.log('\nSKIPPED');
  const byReason = new Map<string, string[]>();
  for (const entry of skipped) {
    const list = byReason.get(entry.reason) ?? [];
    list.push(entry.key);
    byReason.set(entry.reason, list);
  }
  for (const [reason, keys] of byReason) {
    console.log(`  ${reason} (${keys.length})`);
    for (const key of keys) console.log(`    ${key}`);
  }

  if (plan.length !== countOperations(doc)) {
    console.error(`\nFAIL: ${countOperations(doc)} operations, ${plan.length} entries`);
    process.exit(1);
  }
  const uncovered = Object.keys(doc.paths).filter((p) => !plan.some((entry) => entry.path === p));
  if (uncovered.length > 0) {
    console.error(`\nFAIL: no entry for ${uncovered.join(', ')}`);
    process.exit(1);
  }
  console.log(`\nOK: one entry per operation, every path covered.`);
}

if (import.meta.main) main();
