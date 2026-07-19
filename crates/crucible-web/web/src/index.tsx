/* @refresh reload */
import { render } from 'solid-js/web';
import App from './App';
// IBM Plex before index.css (see the note there: CSS @imports of fontsource
// under tailwind v4 lose their font assets; JS imports emit them correctly).
import '@fontsource/ibm-plex-sans/400.css';
import '@fontsource/ibm-plex-sans/500.css';
import '@fontsource/ibm-plex-sans/600.css';
import '@fontsource/ibm-plex-sans/700.css';
import '@fontsource/ibm-plex-mono/400.css';
import '@fontsource/ibm-plex-mono/500.css';
import './index.css';
import { initializeHighlighter } from '@/lib/shiki';

const root = document.getElementById('root');

if (!root) {
  throw new Error('Root element not found');
}

// Fire-and-forget: kick off Shiki download in parallel with first paint.
// DiffViewer reads the reactive `highlighter()` accessor and markdown awaits
// initializeHighlighter(); both fall back to plain text until the promise
// resolves, so this is non-blocking. Surface init failures (offline, corrupt
// WASM, etc.) so diffs/markdown silently degrading to plain text doesn't go
// unnoticed.
void initializeHighlighter().catch((err) => {
  console.error('Shiki highlighter init failed:', err);
});

// PWA service worker: production builds only. The virtual module is emitted
// by vite-plugin-pwa at build time; the PROD guard keeps dev and vitest from
// ever touching it. autoUpdate registration — new builds activate on reload.
if (import.meta.env.PROD) {
  void import('virtual:pwa-register')
    .then(({ registerSW }) => registerSW({ immediate: true }))
    .catch((err) => {
      console.error('Service worker registration failed:', err);
    });
}

render(() => <App />, root);
