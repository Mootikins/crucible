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
// DiffViewer and markdown both call getHighlighter() and fall back to plain
// text until the promise resolves, so this is non-blocking.
void initializeHighlighter();

render(() => <App />, root);
