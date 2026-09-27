import type { Component } from 'solid-js';

// One lazy bundle; individual dynamic imports become thousands of PWA precache requests.
export const icons = import.meta.glob<Component<{ size?: number; 'aria-label'?: string }>>(['/node_modules/lucide-solid/dist/source/icons/*.jsx', '!/node_modules/lucide-solid/dist/source/icons/index.jsx'], { eager: true, import: 'default' });
