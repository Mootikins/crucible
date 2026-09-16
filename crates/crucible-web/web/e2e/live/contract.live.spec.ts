import { test, expect, request as playwrightRequest, type APIRequestContext } from '@playwright/test';
import Ajv2020, { type ValidateFunction } from 'ajv/dist/2020.js';
import path from 'node:path';
import { readState } from './_state';
import {
  buildPlan,
  loadDocument,
  operationKeys,
  type ExercisedOperation,
  type PlannedOperation,
  type ProbeValues,
} from '../../scripts/gen-contract-specs';

/**
 * One live request per contract operation.
 *
 * The other live specs each assert a story. This one asserts the CONTRACT: the
 * committed `openapi.json` says what every route answers, and a running daemon
 * either agrees or does not. The sweep reads the document at run time and
 * builds its tests from it, so a route added in Rust regenerates the document
 * and the next live run has one more test — no file to remember to write.
 *
 * Two assertions per exercised operation, and neither is "it responded":
 *
 *  1. The status is one the document LISTS for that operation. An undocumented
 *     status is a contract defect whether the route works or not, because a
 *     generated client has no branch for it.
 *  2. A 200 JSON body validates against the operation's own 200 schema, with
 *     `ajv` on the document's draft 2020-12 schemas. A missing required field
 *     is what the hand-written `lib/api.ts` casts could never see.
 *
 * Every operation the sweep cannot drive gets a `test.skip` carrying its
 * reason, so the count of tests equals the count of operations and a route
 * cannot fall out of the tier by being awkward.
 */
const state = readState();
const doc = loadDocument();

/**
 * Stands in for the session id while the tests are DECLARED, because a session
 * cannot exist before the daemon is asked for one. The request substitutes the
 * real id. It carries no character `encodeURIComponent` would rewrite, so the
 * substitution is textual and the URL the plan printed is the URL sent.
 */
const SESSION_SENTINEL = 'CONTRACTSWEEPSESSION';

const kilnDir = state.kilnDir ?? '/live-tier-unavailable';
const values: ProbeValues = {
  sessionId: SESSION_SENTINEL,
  kilnDir,
  // `Seed.md` is what globalSetup writes into the kiln and indexes.
  filePath: path.join(kilnDir, 'Seed.md'),
  noteName: 'Seed',
  // Nothing writes this file. A 404 is a documented answer, and the point is
  // that the route answers a documented one.
  canvasPath: path.join(kilnDir, 'Probe.canvas'),
  projectPath: kilnDir,
  searchTerm: 'seed',
  pluginId: 'contract-sweep',
  skillName: 'contract-sweep-probe',
};

const plan: PlannedOperation[] = buildPlan(doc, values);

/**
 * Routes whose live answer contradicts the document.
 *
 * A defect belongs here with the exact failure, never a loosened assertion:
 * `test.fail` keeps the run's verdict honest while the tier stays usable, and
 * turns green the moment the route or the document is fixed, which a deleted
 * assertion never would.
 */
const KNOWN_DEFECTS: Record<string, string> = {
  // ajv, against the running daemon: `data/origins must be object`.
  //
  // The document declares `ConfigResponse.origins` an object. The route
  // answers an ARRAY of origin rows, which is what its own field comment
  // ("One row per recorded leaf") and its own Rust test
  // (`routes/config.rs:224`, `assert!(parsed.origins.is_array())`) say it is.
  // The field is a `serde_json::Value` (`routes/config.rs:42`), and utoipa
  // projects a bare `Value` as `type: object`, so the document says the one
  // thing the route never sends. A generated client reading `origins` gets an
  // object type and an array at run time.
  //
  // The fix belongs to the config route, not to this sweep: give the field a
  // declared `value_type` and regenerate the document.
  'GET /api/config':
    'ConfigResponse.origins is declared an object and answered as an array of origin rows',
};

const ajv = new Ajv2020({
  // The document is OpenAPI 3.1, so its schemas ARE draft 2020-12 — but the
  // wrapper carries `components`, which is not a schema keyword.
  strict: false,
  // `int32`, `int64`, `float`, `double` and `date-time` are OpenAPI formats,
  // not assertions. Validating them would need `ajv-formats` and would say
  // nothing about the shape, which is what this sweep is for.
  validateFormats: false,
  allErrors: true,
});
ajv.addSchema({ $id: 'crucible://openapi', components: doc.components });

const validators = new Map<string, ValidateFunction>();
function validatorFor(ref: string): ValidateFunction {
  const cached = validators.get(ref);
  if (cached) return cached;
  const compiled = ajv.compile({ $ref: ref.replace('#/', 'crucible://openapi#/') });
  validators.set(ref, compiled);
  return compiled;
}

let api: APIRequestContext;
let sessionId = '';

test.describe('the live server answers its own contract', () => {
  test.skip(state.skip, `live tier unavailable: ${state.reason ?? ''}`);

  test.beforeAll(async () => {
    api = await playwrightRequest.newContext({ baseURL: state.baseURL });
    // The sweep's own session, so `{id}` is a real id rather than a probe the
    // daemon would refuse for a reason that has nothing to do with the route.
    const created = await api.post('/api/session', {
      data: { session_type: 'chat', kilns: [], agent_type: 'internal' },
    });
    expect(created.status(), await created.text()).toBe(200);
    sessionId = ((await created.json()) as { session_id: string }).session_id;
  });

  test.afterAll(async () => {
    await api?.dispose();
  });

  test('the plan carries one entry per operation, and every path is covered', () => {
    // Keys, not entries: a mismatch then reads as the operation that gained or
    // lost a test, rather than as a dump of the whole plan.
    const planned = plan.map((entry) => entry.key).sort();
    expect(planned, 'an operation gained or lost its test').toEqual(operationKeys(doc).sort());
    expect(new Set(planned).size, 'two entries share a key').toBe(plan.length);

    const uncovered = Object.keys(doc.paths).filter((p) => !plan.some((entry) => entry.path === p));
    expect(uncovered, 'a path in the document has no test').toEqual([]);

    const unfilled = plan
      .filter((entry): entry is ExercisedOperation => entry.kind === 'exercise')
      .filter((entry) => entry.url.includes('{'))
      .map((entry) => entry.key);
    expect(unfilled, 'a request kept an unfilled path parameter').toEqual([]);
  });

  for (const entry of plan) {
    if (entry.kind === 'skip') {
      test.skip(
        `${entry.key} — not swept: ${entry.reason}`,
        { annotation: { type: 'contract-skip', description: entry.reason } },
        () => {},
      );
      continue;
    }

    test(`${entry.key} answers a documented status and its 200 schema`, async () => {
      const defect = KNOWN_DEFECTS[entry.key];
      if (defect) test.fail(true, defect);

      const url = entry.url.replace(SESSION_SENTINEL, sessionId);
      const res = await api.get(url, { headers: entry.headers });
      const status = res.status();
      const body = await res.text();

      expect(
        entry.documented,
        `${entry.key} answered ${status}, which the document does not list. Body: ${body.slice(0, 400)}`,
      ).toContain(status);

      if (status !== 200) return;
      // A non-JSON 200 has no schema to check: `/api/file/raw` hands over the
      // bytes it was asked for.
      if (entry.okMediaType !== 'application/json') return;

      let parsed: unknown;
      try {
        parsed = JSON.parse(body);
      } catch {
        throw new Error(`${entry.key} answered 200 with a body that is not JSON: ${body.slice(0, 400)}`);
      }
      // `GET /api/layout` declares a JSON 200 with no schema, so parsing it is
      // the whole assertion the document supports.
      if (!entry.okSchemaRef) return;

      const validate = validatorFor(entry.okSchemaRef);
      const valid = validate(parsed);
      expect(
        valid,
        `${entry.key} answered 200 with a body that does not match ${entry.okSchemaRef}: ` +
          `${ajv.errorsText(validate.errors, { separator: '; ' })}. Body: ${body.slice(0, 400)}`,
      ).toBe(true);
    });
  }
});
