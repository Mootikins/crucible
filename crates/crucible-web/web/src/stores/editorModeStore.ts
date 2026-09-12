import { createSignal } from 'solid-js';

/** How the compact shell draws a markdown note: reading it, or writing it. */
export type CompactEditorMode = 'reading' | 'live';

/**
 * The compact shell's editor mode, lifted out of the editor.
 *
 * The desktop editor toggles itself with two small floating buttons. They sit
 * over the text and are far under a thumb's target size, so on a phone the app
 * bar carries a Read/Write control instead — and the mode has to live where
 * both the bar and the editor can reach it.
 */
const [compactEditorMode, setCompactEditorMode] = createSignal<CompactEditorMode>('live');

export { compactEditorMode, setCompactEditorMode };
