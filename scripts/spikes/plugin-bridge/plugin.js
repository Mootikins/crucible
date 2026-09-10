// SPIKE ONLY - not app code.
//
// The PLUGIN half. This is third-party code: an opaque origin, no cookie, no
// app DOM, no network. Everything it can do arrives on one port.

const S = document.getElementById('s');
const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const p95 = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length * 0.95)];

window.addEventListener('message', (e) => {
  // e.origin is the string "null" for every sandboxed frame, so it proves
  // nothing. The port is the capability.
  const port = e.ports[0];
  if (!port) return;
  start(port, e.data);
});

function makeCall(port) {
  let seq = 0;
  const pending = new Map();
  port.onmessage = (e) => {
    const { id, ok, value, error } = e.data ?? {};
    const slot = pending.get(id);
    if (!slot) return;
    pending.delete(id);
    ok ? slot.resolve(value) : slot.reject(new Error(`${error.code}: ${error.message}`));
  };
  return (method, params) =>
    new Promise((resolve, reject) => {
      const id = ++seq;
      pending.set(id, { resolve, reject });
      port.postMessage({ v: 1, id, method, params });
    });
}

async function timeIt(fn, n, warm) {
  const samples = [];
  for (let i = 0; i < n + warm; i++) {
    const t0 = performance.now();
    await fn();
    const dt = performance.now() - t0;
    if (i >= warm) samples.push(dt);
  }
  return { median: +median(samples).toFixed(3), p95: +p95(samples).toFixed(3), n: samples.length };
}

async function start(port, cfg) {
  const call = makeCall(port);
  S.textContent = 'measuring…';

  if (cfg.quiet) {
    // Mount-cost mode: no probing, no measuring. Just say the port is live.
    S.textContent = 'ready';
    port.postMessage({ v: 1, method: 'report', params: { ready: true } });
    return;
  }

  // Does the containment actually hold? Try what a hostile block would try.
  const escapes = {};
  try {
    const r = await fetch('/api/plugins/publications?key=kanban:board');
    escapes.direct_fetch = `SUCCEEDED (http ${r.status}) - containment is broken`;
  } catch (err) {
    escapes.direct_fetch = `refused: ${String(err).slice(0, 90)}`;
  }
  try {
    escapes.parent_dom = window.parent.document.title ? 'READABLE' : 'empty';
  } catch (err) {
    escapes.parent_dom = `refused: ${String(err).slice(0, 60)}`;
  }
  try {
    escapes.cookie = document.cookie === '' ? 'empty' : 'READABLE';
  } catch (err) {
    escapes.cookie = `refused: ${String(err).slice(0, 60)}`;
  }
  try {
    localStorage.getItem('x');
    escapes.local_storage = 'READABLE';
  } catch (err) {
    escapes.local_storage = `refused: ${String(err).slice(0, 60)}`;
  }
  escapes.origin = window.origin;

  if (cfg.escapesOnly) {
    S.textContent = 'escape probe done';
    port.postMessage({ v: 1, method: 'report', params: { escapes } });
    return;
  }

  const results = {
    escapes,
    'bridge only (ping, no fetch)': await timeIt(() => call('ping', {}), cfg.n, cfg.warm),
    'bridged publications': await timeIt(
      () => call('publications.get', { key: cfg.key }),
      cfg.n,
      cfg.warm,
    ),
    'bridged 2k-note graph': await timeIt(() => call('bulk.get', { n: 2000 }), cfg.n, cfg.warm),
    refusal: await call('kanban_move', {}).then(
      () => 'ALLOWED - the table is not closed',
      (e) => e.message,
    ),
  };

  S.textContent = 'done';
  port.postMessage({ v: 1, method: 'report', params: results });
}
