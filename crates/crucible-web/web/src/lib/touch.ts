import { isCompact } from '@/stores/deviceStore';

/**
 * The class list that makes a control a 44 px touch target on the compact
 * shell, and nothing on the desktop shell. The desktop keeps a dense row; a
 * finger on a phone needs the target the compact shell already gives its own
 * buttons (`MobileShell.tsx`, `w-11 h-11`). The shell is decided once at page
 * load, so a call at render time is stable for the life of the page.
 */
export const hit = () => (isCompact() ? 'min-w-11 min-h-11' : '');
