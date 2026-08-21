import {
  Component,
  createEffect,
  createSignal,
  onCleanup,
  Match,
  Show,
  Switch,
} from 'solid-js';
import { Terminal } from '@xterm/xterm';
import { FitAddon } from '@xterm/addon-fit';
import { WebglAddon } from '@xterm/addon-webgl';
import { Unicode11Addon } from '@xterm/addon-unicode11';
import '@xterm/xterm/css/xterm.css';
import { terminalAllowed, terminalDenied } from '@/lib/terminal-availability';
import { nextReconnectDelay } from '@/lib/terminal-backoff';
import { statusBarStore } from '@/stores/statusBarStore';
import { useSettingsSafe } from '@/contexts/SettingsContext';

/**
 * Real terminal: xterm.js over the daemon's PTY WebSocket
 * (`/api/terminal/ws`, localhost-only). Server sends raw PTY bytes as
 * binary frames; we send JSON text frames — `{t:'i',d}` input,
 * `{t:'r',cols,rows}` resize.
 */

// Ember-shell ANSI theme. xterm renders into a canvas/DOM layer that can't
// consume CSS custom properties, so every entry is READ from the design
// tokens at mount (single source of truth: the --color-term-* ramp in
// index.css), with the token's literal as fallback. The ANSI ramp is
// deliberately its own token family — not the UI semantic tokens.
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
    black: v('--color-term-black', '#2b2933'),
    red: v('--color-term-red', '#e8746e'),
    green: v('--color-term-green', '#9dcf85'),
    yellow: v('--color-term-yellow', '#e0b24c'),
    blue: v('--color-term-blue', '#7fa7e0'),
    magenta: v('--color-term-magenta', '#bd93e0'),
    cyan: v('--color-term-cyan', '#79c9c4'),
    white: v('--color-term-white', '#c9c5bf'),
    brightBlack: v('--color-term-bright-black', '#6b6673'),
    brightRed: v('--color-term-bright-red', '#f2938c'),
    brightGreen: v('--color-term-bright-green', '#b7e0a1'),
    brightYellow: v('--color-term-bright-yellow', '#ecc76e'),
    brightBlue: v('--color-term-bright-blue', '#a0c0ee'),
    brightMagenta: v('--color-term-bright-magenta', '#d0b0ee'),
    brightCyan: v('--color-term-bright-cyan', '#9adcd7'),
    brightWhite: v('--color-term-bright-white', '#e7e4df'),
  };
}

/**
 * The PTY socket, carrying the workspace the user is looking at.
 *
 * The server cannot work this out for itself: it used to start the shell in its
 * OWN working directory on the theory that this is "the project you launched
 * from", which is `$HOME` for the installed systemd unit — so a terminal opened
 * from a project landed in the home directory. Only the client knows which root
 * is focused, so only the client can say.
 *
 * The WORKSPACE, not the browsed root. This used to follow the file tree,
 * which was one app-wide root; the tree now follows the session and can be
 * pointed at any attached kiln, and a shell has no business moving into a
 * notes corpus because you opened it to read. A terminal runs commands where
 * the session acts.
 */
function wsUrl(): string {
  const proto = window.location.protocol === 'https:' ? 'wss' : 'ws';
  const base = `${proto}://${window.location.host}/api/terminal/ws`;
  const path = statusBarStore.workspacePath();
  return path ? `${base}?cwd=${encodeURIComponent(path)}` : base;
}

export const TerminalPanel: Component = () => {
  const [status, setStatus] = createSignal<'connecting' | 'open' | 'closed' | 'reconnecting'>(
    'connecting',
  );
  // Terminal font: its own setting when set, else the Appearance code font,
  // else the shell default. xterm renders to canvas, so CSS vars can't reach
  // it — the resolved family is passed as a real option.
  const { settings } = useSettingsSafe();
  const termFontFamily = () =>
    settings.terminal.fontFamily.trim() ||
    settings.appearance.fontMono.trim() ||
    "'IBM Plex Mono', ui-monospace, monospace";
  const termFontSize = () => Math.max(8, settings.terminal.fontSize || 13);
  // The PTY endpoint is gated server-side to localhost (a PTY is full shell
  // access) unless the server opted into authenticated remote access — the
  // shared availability module does the /api/config round-trip once, so a
  // LAN client either connects or gets an honest explanation instead of a
  // dead reconnect loop.
  const allowed = terminalAllowed;
  const denied = terminalDenied;
  let container: HTMLDivElement | undefined;
  let term: Terminal | undefined;
  let fitAddon: FitAddon | undefined;
  let socket: WebSocket | undefined;
  let resizeObserver: ResizeObserver | undefined;
  let reconnectAttempts = 0;
  let reconnectTimer: ReturnType<typeof setTimeout> | null = null;
  let disposed = false;

  // Live-apply font changes from Settings to the running terminal.
  createEffect(() => {
    const family = termFontFamily();
    const size = termFontSize();
    if (!term) return;
    term.options.fontFamily = family;
    term.options.fontSize = size;
    try {
      fitAddon?.fit();
    } catch {
      // Fitting a zero-sized (hidden) panel throws; harmless.
    }
  });

  /**
   * Schedule the next attempt, or don't.
   *
   * A dropped socket used to be terminal (in both senses): status went to
   * `closed` and stayed there until someone clicked. Every server restart —
   * including every `just install`, which overwrites the binary the unit runs —
   * therefore left an open tab showing "Session ended" forever, while ordinary
   * HTTP requests recovered silently and made the rest of the UI look healthy.
   */
  const scheduleReconnect = () => {
    if (disposed || reconnectTimer) return;
    // A hidden tab retrying on a timer is pure waste; the visibility listener
    // below re-arms the moment it comes back.
    if (document.hidden) {
      setStatus('closed');
      return;
    }
    setStatus('reconnecting');
    const delay = nextReconnectDelay(reconnectAttempts++);
    reconnectTimer = setTimeout(() => {
      reconnectTimer = null;
      if (!disposed && term) connect(term);
    }, delay);
  };

  const connect = (t: Terminal) => {
    setStatus('connecting');
    const ws = new WebSocket(wsUrl());
    ws.binaryType = 'arraybuffer';
    socket = ws;

    ws.onopen = () => {
      // Reset here, not on the attempt: a socket that opens and immediately
      // dies must keep escalating, or a half-broken server gets hammered at
      // the base delay forever.
      reconnectAttempts = 0;
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
    ws.onclose = () => scheduleReconnect();
    ws.onerror = () => scheduleReconnect();

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
  };

  /**
   * Try again NOW, discarding a scheduled attempt and the escalation behind it.
   *
   * Backoff protects a server that is down; it actively hurts the far more
   * common case where the server came back and the client is still counting
   * down. After a long outage the delay sits at the 15s cap, so a terminal whose
   * server recovered took up to 15s to notice — and reported as "took forever",
   * because the wait is dead time in front of a working shell.
   *
   * Focus, visibility and `online` are all evidence worth acting on: the first
   * two mean the user is looking at this panel, the third that the network
   * changed. None of them proves the server is up, so a failed retry simply
   * re-enters the backoff from zero.
   */
  const retryNow = () => {
    if (disposed || document.hidden) return;
    if (status() === 'open' || status() === 'connecting') return;
    if (reconnectTimer) {
      clearTimeout(reconnectTimer);
      reconnectTimer = null;
    }
    reconnectAttempts = 0;
    if (term) connect(term);
  };

  const init = (el: HTMLDivElement) => {
    container = el;
    const t = new Terminal({
      theme: buildEmberTheme(),
      fontFamily: termFontFamily(),
      fontSize: termFontSize(),
      cursorBlink: true,
      scrollback: 5000,
      // VS Code-parity fidelity options. customGlyphs vector-draws
      // box-drawing and powerline separators (U+E0B0…) instead of trusting
      // the font — the fix for seams/gaps between powerline segments — but
      // only takes effect on the WebGL renderer, loaded below. The 4.5
      // contrast floor nudges unreadable fg/bg pairs apart (xterm exempts
      // powerline glyphs from the nudge, so segments keep exact colors).
      customGlyphs: true,
      minimumContrastRatio: 4.5,
      drawBoldTextInBrightColors: true,
      allowProposedApi: true,
      // WebGL paints to canvas — without this there is no DOM text at all,
      // for screen readers or tests. The a11y layer mirrors visible rows
      // as hidden DOM text.
      screenReaderMode: true,
    });
    const fit = new FitAddon();
    t.loadAddon(fit);
    // Unicode 11 width tables: emoji/Nerd icon glyphs get correct cell
    // widths so prompt columns don't drift (needs allowProposedApi).
    t.loadAddon(new Unicode11Addon());
    t.unicode.activeVersion = '11';
    term = t;
    fitAddon = fit;
    // Defer to the next frame: the panel container has zero size until the
    // layout pass, and xterm measures on open().
    requestAnimationFrame(() => {
      if (!container) return;
      t.open(container);
      // GPU renderer — also the renderer where customGlyphs is live (the
      // DOM renderer ignores it). Falls back to DOM if WebGL is
      // unavailable or the context is lost.
      try {
        const webgl = new WebglAddon();
        webgl.onContextLoss(() => webgl.dispose());
        t.loadAddon(webgl);
      } catch {
        // WebGL unavailable (headless/old GPU) — DOM renderer still works.
      }
      fit.fit();
      connect(t);
    });
    document.addEventListener('visibilitychange', retryNow);
    window.addEventListener('focus', retryNow);
    window.addEventListener('online', retryNow);
    resizeObserver = new ResizeObserver(() => {
      try {
        fit.fit();
      } catch {
        // Fitting a zero-sized (hidden) panel throws; harmless.
      }
    });
    resizeObserver.observe(el);
  };

  /** The manual escape hatch: same reset as `retryNow`, plus a screen wipe. */
  const reconnect = () => {
    if (!term) return;
    term.reset();
    // Reuse the addon loaded in `init`. Constructing one per attempt leaked:
    // xterm's addon manager holds every loaded addon until `Terminal.dispose()`,
    // and `reset()` clears the buffer, not addons — so N reconnects left N live
    // FitAddons all subscribed to resize/render, with `fitAddon` pointing only
    // at the newest.
    try {
      fitAddon?.fit();
    } catch {
      // Fitting a zero-sized (hidden) panel throws; harmless.
    }
    retryNow();
  };

  onCleanup(() => {
    // Before closing the socket: `onclose` fires during teardown, and without
    // this it would schedule a reconnect against a disposed terminal.
    disposed = true;
    if (reconnectTimer) clearTimeout(reconnectTimer);
    document.removeEventListener('visibilitychange', retryNow);
    window.removeEventListener('focus', retryNow);
    window.removeEventListener('online', retryNow);
    resizeObserver?.disconnect();
    socket?.close();
    term?.dispose();
  });

  return (
    <Switch
      fallback={
        // Neither allowed nor denied: the availability check is in flight, or
        // it failed and left the answer unknown. This was a blank panel, which
        // is indistinguishable from a broken one.
        <div
          class="h-full w-full bg-shell-bg flex flex-col items-center justify-center gap-1.5 px-6 text-center"
          data-testid="terminal-panel"
        >
          <span class="text-sm text-muted-dark" data-testid="terminal-checking">
            Checking terminal availability…
          </span>
        </div>
      }
    >
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
            under [web] in config.toml). An API key must be configured. If the terminal still
            refuses after that, this host is not one the server answers to — add "
            {window.location.host}" to allowed_hosts under [web].
          </span>
        </div>
      </Match>
      <Match when={allowed()}>
        <div class="relative h-full w-full bg-shell-bg" data-testid="terminal-panel">
          <div ref={init} class="h-full w-full pl-2 pt-1" />
          <Show when={status() === 'closed' || status() === 'reconnecting'}>
            {/* z-20: xterm's accessibility layer is z-10 inside the term —
                the overlay must stay clickable above it. */}
            <div class="absolute inset-0 z-20 flex items-center justify-center bg-shell-bg/80 cru-anim-fade">
              <button
                type="button"
                data-testid="terminal-reconnect"
                onClick={reconnect}
                class="px-3 py-1.5 rounded border border-hairline-strong bg-control text-shell-ink text-sm hover:bg-hover-wash transition-colors"
              >
                {status() === 'reconnecting' ? 'Reconnecting… — retry now' : 'Session ended — reconnect'}
              </button>
            </div>
          </Show>
        </div>
      </Match>
    </Switch>
  );
};
