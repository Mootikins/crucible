// SPIKE ONLY - not app code.
//
// The HOST half of the bridge. It owns the identity: it created the frame, so
// it knows which plugin the frame draws for, and it stamps that name on every
// outbound request. The frame never says who it is.

const OUT = document.getElementById('out');
const N = 60;
const WARM = 10;
const KEY = 'kanban:board';

const median = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length / 2)];
const p95 = (xs) => [...xs].sort((a, b) => a - b)[Math.floor(xs.length * 0.95)];

async function timeIt(fn) {
  const samples = [];
  for (let i = 0; i < N + WARM; i++) {
    const t0 = performance.now();
    await fn();
    const dt = performance.now() - t0;
    if (i >= WARM) samples.push(dt);
  }
  return { median: +median(samples).toFixed(3), p95: +p95(samples).toFixed(3), n: samples.length };
}

// ---- the host-side method table. This IS the plugin-facing surface. --------
// Every entry takes the CALLER (stamped by the host, not sent by the frame)
// and the params. Anything not in this table is refused.
function methodsFor(plugin) {
  return {
    ping: async () => ({ pong: true }),
    'publications.get': async ({ key }) => {
      const r = await fetch(`/api/plugins/publications?key=${encodeURIComponent(key)}`, {
        headers: { 'X-Crucible-Plugin': plugin }, // <- stamped here
      });
      if (!r.ok) throw new Error(`http ${r.status}`);
      const body = await r.json();
      return body.publications?.[key]?.[plugin] ?? null;
    },
    'bulk.get': async ({ n }) => {
      const r = await fetch(`/bulk?n=${n}`);
      return await r.json();
    },
  };
}

function mountPlugin(plugin, onReport, src = '/frame.html', escapesOnly = false, parent = document.body) {
  const frame = document.createElement('iframe');
  // Belt: the embedder's own sandbox attribute. No allow-same-origin, so the
  // document lands in an opaque origin whatever the server said.
  frame.setAttribute('sandbox', 'allow-scripts');
  frame.src = src;
  frame.style.cssText = 'width:400px;height:120px;border:1px solid #999';
  parent.appendChild(frame);

  const table = methodsFor(plugin);
  const channel = new MessageChannel();

  channel.port1.onmessage = async (e) => {
    const msg = e.data;
    if (msg?.method === 'report') {
      onReport(msg.params);
      return;
    }
    const { v, id, method, params } = msg ?? {};
    const reply = (body) => channel.port1.postMessage({ v: 1, id, ...body });
    if (v !== 1 || typeof id !== 'number') return; // unaddressable: drop
    const fn = table[method];
    if (!fn) return reply({ ok: false, error: { code: 'unknown_method', message: method } });
    try {
      reply({ ok: true, value: await fn(params ?? {}) });
    } catch (err) {
      reply({ ok: false, error: { code: 'call_failed', message: String(err?.message ?? err) } });
    }
  };

  frame.addEventListener('load', () => {
    // targetOrigin MUST be '*': an opaque origin cannot be named. The frame is
    // identified by holding this exact contentWindow, and thereafter by the
    // private port, never by event.origin (which is the string "null").
    frame.contentWindow.postMessage({ v: 1, hello: plugin, n: N, warm: WARM, key: KEY, escapesOnly, quiet: escapesOnly === 'quiet' }, '*', [
      channel.port2,
    ]);
  });
}

// `?frames=N` — how a note holding N blocks behaves. One iframe per block.
async function frameScaling(counts) {
  const out = {};
  for (const k of counts) {
    const holder = document.createElement('div');
    document.body.appendChild(holder);
    const t0 = performance.now();
    await Promise.all(
      Array.from({ length: k }, () =>
        new Promise((resolve) => mountPlugin('kanban', resolve, '/quiet.html', 'quiet', holder)),
      ),
    );
    out[`${k} blocks`] = {
      total_ms: +(performance.now() - t0).toFixed(1),
      per_block_ms: +((performance.now() - t0) / k).toFixed(2),
      heap_mb: performance.memory ? +(performance.memory.usedJSHeapSize / 1048576).toFixed(1) : null,
    };
    holder.remove();
  }
  return out;
}

(async () => {
  const baseline = {
    'direct fetch, publications': await timeIt(async () => {
      const r = await fetch(`/api/plugins/publications?key=${encodeURIComponent(KEY)}`, {
        headers: { 'X-Crucible-Plugin': 'app' },
      });
      await r.json();
    }),
    'direct fetch, 2k-note graph': await timeIt(async () => {
      const r = await fetch('/bulk?n=2000');
      await r.json();
    }),
  };

  const openFrame = await new Promise((resolve) => {
    mountPlugin('kanban', resolve, '/frame-open.html', true);
  });

  mountPlugin('kanban', async (framed) => {
    OUT.textContent = JSON.stringify(
      {
        baseline,
        framed,
        'opaque origin, no connect-src': openFrame.escapes,
        'frames per note': await frameScaling([1, 4, 16, 32]),
      },
      null,
      2,
    );
    document.title = 'done';
  });
})();
