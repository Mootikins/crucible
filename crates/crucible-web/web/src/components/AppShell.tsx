import type { Component } from 'solid-js';
import { WindowManager } from '@/components/windowing/WindowManager';
import { MobileShell } from '@/components/mobile/MobileShell';
import { isCompact } from '@/stores/deviceStore';

/**
 * The one shell this page draws. Never both: the two shells hold different tab
 * stores, and two live editors on one file would each claim its dirty state.
 * `isCompact()` is fixed at load, so this choice never flips under a user.
 */
export const AppShell: Component = () => (isCompact() ? <MobileShell /> : <WindowManager />);
