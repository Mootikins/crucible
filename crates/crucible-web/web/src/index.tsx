/* @refresh reload */
import { render } from 'solid-js/web';
import App from './App';
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

render(() => <App />, root);
