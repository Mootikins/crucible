import { Component, createResource, createSignal, onCleanup, Match, Show, Switch } from 'solid-js';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import '@xterm/xterm/css/xterm.css';
import { getConfig } from '@/lib/api';

/**
 * Real terminal: xterm.js over the daemon's PTY WebSocket
 * (`/api/terminal/ws`, localhost-only). Server sends raw PTY bytes as
 * binary frames; we send JSON text frames — `{t:'i',d}` input,
 * `{t:'r',cols,rows}` resize.
 */

// Ember-shell ANSI theme. xterm renders into a canvas/DOM layer that can't
// consume CSS custom properties, so the tokened entries are READ from the
// design tokens at mount (single source of truth in index.css); each falls
// back to its literal if the var is unresolved. The two untokened ANSI slots
// (blue, cyan) stay as literals.
function buildEmberTheme() {
  const css = getComputedStyle(document.documentElement);
  const v = (name: string, fallback: string) => css.getPropertyValue(name).trim() || fallback;
  // Match the dock chrome (EdgePanel content is bg-shell-bg) — shell-panel
  // here made the terminal render as a visibly lighter rectangle.
  const bg = v('--color-shell-bg', '#0e0d11');
  return {
    background: bg,
    foreground: v('--color-shell-ink', '#e7e4df'),
    cursor: v('--color-primary', '#e0653a'),
    cursorAccent: bg,
    selectionBackground: 'rgba(224, 101, 58, 0.35)',
    black: '#26252b',
    red: v('--color-error', '#ef4444'),
    green: v('--color-ok', '#7bc47f'),
    yellow: v('--color-attention', '#d4a72c'),
    blue: '#7aa2f7',
    magenta: v('--color-precog', '#a78bda'),
    cyan: '#76c7c0',
    white: '#c9c5bf',
    brightBlack: '#6b6673',
    brightRed: '#f87171',
    brightGreen: '#9ed9a2',
    brightYellow: '#e3bd52',
    brightBlue: '#9db8f9',
    brightMagenta: '#c1a8ee',
    brightCyan: '#98dbd5',
    brightWhite: '#e7e4df',
  };
}

function wsUrl(): string {
  const proto = window.location.protocol === 'https:' ? 'wss' : 'ws';
  return `${proto}://${window.location.host}/api/terminal/ws`;
}

// The PTY endpoint is gated server-side to localhost (a PTY is full shell
// access) unless the server opted into authenticated remote access
// (`cru web --remote-shell` / `[server] remote_shell`, requires an API
// key). /api/config reports whether the opt-in is active, so a LAN client
// either connects or gets an honest explanation instead of a dead
// reconnect loop.
function isLocalhost(): boolean {
  const h = window.location.hostname;
  return h === 'localhost' || h === '127.0.0.1' || h === '[::1]' || h === '::1';
}

export const TerminalPanel: Component = () => {
  const [status, setStatus] = createSignal<'connecting' | 'open' | 'closed'>('connecting');
  // Localhost never needs the config round-trip; remote clients check the
  // server's remote-shell opt-in before attempting the socket.
  const [cfg] = createResource(
    () => (isLocalhost() ? null : true),
    () => getConfig().catch(() => ({ kiln_path: '', remote_shell: false })),
  );
  const allowed = () => isLocalhost() || cfg()?.remote_shell === true;
  const denied = () => !isLocalhost() && cfg() !== undefined && cfg()?.remote_shell !== true;
  let container: HTMLDivElement | undefined;
  let term: Terminal | undefined;
  let socket: WebSocket | undefined;
  let resizeObserver: ResizeObserver | undefined;

  const connect = (t: Terminal, fit: FitAddon) => {
    setStatus('connecting');
    const ws = new WebSocket(wsUrl());
    ws.binaryType = 'arraybuffer';
    socket = ws;

    ws.onopen = () => {
      setStatus('open');
      // Sync the PTY to the fitted size before the first prompt paints.
      ws.send(JSON.stringify({ t: 'r', cols: t.cols, rows: t.rows }));
      t.focus();
    };
    ws.onmessage = (ev) => {
      if (typeof ev.data === 'string') {
        t.write(ev.data);
      } else {
        t.write(new Uint8Array(ev.data as ArrayBuffer));
      }
    };
    ws.onclose = () => setStatus('closed');
    ws.onerror = () => setStatus('closed');

    const dataSub = t.onData((d) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ t: 'i', d }));
      }
    });
    const resizeSub = t.onResize(({ cols, rows }) => {
      if (ws.readyState === WebSocket.OPEN) {
        ws.send(JSON.stringify({ t: 'r', cols, rows }));
      }
    });
    ws.addEventListener('close', () => {
      dataSub.dispose();
      resizeSub.dispose();
    });
    void fit;
  };

  const init = (el: HTMLDivElement) => {
    container = el;
    const t = new Terminal({
      theme: buildEmberTheme(),
      fontFamily: "'IBM Plex Mono', ui-monospace, monospace",
      fontSize: 13,
      cursorBlink: true,
      scrollback: 5000,
    });
    const fit = new FitAddon();
    t.loadAddon(fit);
    term = t;
    // Defer to the next frame: the panel container has zero size until the
    // layout pass, and xterm measures on open().
    requestAnimationFrame(() => {
      if (!container) return;
      t.open(container);
      fit.fit();
      connect(t, fit);
    });
    resizeObserver = new ResizeObserver(() => {
      try {
        fit.fit();
      } catch {
        // Fitting a zero-sized (hidden) panel throws; harmless.
      }
    });
    resizeObserver.observe(el);
  };

  const reconnect = () => {
    if (!term) return;
    term.reset();
    const fit = new FitAddon();
    term.loadAddon(fit);
    connect(term, fit);
  };

  onCleanup(() => {
    resizeObserver?.disconnect();
    socket?.close();
    term?.dispose();
  });

  return (
    <Switch fallback={<div class="h-full w-full bg-shell-bg" data-testid="terminal-panel" />}>
      <Match when={denied()}>
        <div
          class="h-full w-full bg-shell-bg flex flex-col items-center justify-center gap-1.5 px-6 text-center"
          data-testid="terminal-panel"
        >
          <span class="text-sm text-shell-body">
            Terminal is only available from the host machine
          </span>
          <span class="text-xs text-muted-dark max-w-md">
            A terminal is full shell access, so Crucible only serves it to localhost by default —
            you're connected from {window.location.hostname}. To allow authenticated remote
            devices, run the server with `cru web --remote-shell` (or set remote_shell = true
            under [server] in config.toml). An API key must be configured.
          </span>
        </div>
      </Match>
      <Match when={allowed()}>
        <div class="relative h-full w-full bg-shell-bg" data-testid="terminal-panel">
          <div ref={init} class="h-full w-full pl-2 pt-1" />
          <Show when={status() === 'closed'}>
            <div class="absolute inset-0 flex items-center justify-center bg-shell-bg/80 cru-anim-fade">
              <button
                type="button"
                data-testid="terminal-reconnect"
                onClick={reconnect}
                class="px-3 py-1.5 rounded border border-hairline-strong bg-control text-shell-ink text-sm hover:bg-hover-wash transition-colors"
              >
                Session ended — reconnect
              </button>
            </div>
          </Show>
        </div>
      </Match>
    </Switch>
  );
};
