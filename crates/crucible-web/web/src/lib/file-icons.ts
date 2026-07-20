import type { Component, JSX } from 'solid-js';
import {
  File,
  FileText,
  FileCode,
  Braces,
  FileImage,
  FileArchive,
  FileLock,
  Palette,
  Globe,
  Moon,
  Cog,
  Database,
  Terminal,
} from '@/lib/icons';

/** Icon + accent color for a file, keyed by extension. Colors follow the
 * VSCode/seti convention (per-language hues) so a file tree reads at a glance;
 * lucide icons are monochrome, so the color is applied via `style`. */
export interface FileIconMeta {
  icon: Component<{ class?: string; style?: JSX.CSSProperties }>;
  color: string;
}

const BLUE = '#519aba';
const YELLOW = '#cbcb41';
const ORANGE = '#e37933';
const RUST = '#dea584';
const PURPLE = '#a074c4';
const GREEN = '#8dc149';
const GRAY = '#6d8086';
const PINK = '#cc6699';
const RED = '#cc3e44';

const BY_EXT: Record<string, FileIconMeta> = {
  md: { icon: FileText, color: BLUE },
  markdown: { icon: FileText, color: BLUE },
  txt: { icon: FileText, color: '#98939e' },
  ts: { icon: FileCode, color: BLUE },
  tsx: { icon: FileCode, color: BLUE },
  js: { icon: FileCode, color: YELLOW },
  jsx: { icon: FileCode, color: YELLOW },
  mjs: { icon: FileCode, color: YELLOW },
  cjs: { icon: FileCode, color: YELLOW },
  rs: { icon: FileCode, color: RUST },
  py: { icon: FileCode, color: BLUE },
  go: { icon: FileCode, color: '#519aba' },
  rb: { icon: FileCode, color: RED },
  java: { icon: FileCode, color: ORANGE },
  c: { icon: FileCode, color: BLUE },
  h: { icon: FileCode, color: PURPLE },
  cpp: { icon: FileCode, color: BLUE },
  cc: { icon: FileCode, color: BLUE },
  hpp: { icon: FileCode, color: PURPLE },
  php: { icon: FileCode, color: PURPLE },
  lua: { icon: Moon, color: '#51a0cf' },
  fnl: { icon: Moon, color: PURPLE },
  json: { icon: Braces, color: YELLOW },
  jsonc: { icon: Braces, color: YELLOW },
  toml: { icon: Cog, color: GRAY },
  yaml: { icon: Cog, color: PINK },
  yml: { icon: Cog, color: PINK },
  ini: { icon: Cog, color: GRAY },
  conf: { icon: Cog, color: GRAY },
  css: { icon: Palette, color: BLUE },
  scss: { icon: Palette, color: PINK },
  sass: { icon: Palette, color: PINK },
  html: { icon: Globe, color: ORANGE },
  xml: { icon: Globe, color: ORANGE },
  svg: { icon: FileImage, color: ORANGE },
  png: { icon: FileImage, color: PURPLE },
  jpg: { icon: FileImage, color: PURPLE },
  jpeg: { icon: FileImage, color: PURPLE },
  gif: { icon: FileImage, color: PURPLE },
  webp: { icon: FileImage, color: PURPLE },
  ico: { icon: FileImage, color: PURPLE },
  sh: { icon: Terminal, color: GREEN },
  bash: { icon: Terminal, color: GREEN },
  zsh: { icon: Terminal, color: GREEN },
  fish: { icon: Terminal, color: GREEN },
  sql: { icon: Database, color: ORANGE },
  db: { icon: Database, color: GRAY },
  zip: { icon: FileArchive, color: '#98939e' },
  tar: { icon: FileArchive, color: '#98939e' },
  gz: { icon: FileArchive, color: '#98939e' },
  xz: { icon: FileArchive, color: '#98939e' },
  lock: { icon: FileLock, color: GRAY },
};

/** Extensionless config files recognized by full name. */
const BY_NAME: Record<string, FileIconMeta> = {
  dockerfile: { icon: Cog, color: BLUE },
  makefile: { icon: Cog, color: GRAY },
  justfile: { icon: Cog, color: GRAY },
  '.gitignore': { icon: Cog, color: ORANGE },
  '.env': { icon: Cog, color: YELLOW },
};

const DEFAULT: FileIconMeta = { icon: File, color: '#6b6673' };

/** Resolve the colored icon for a filename. */
export function fileIconFor(filename: string): FileIconMeta {
  const lower = filename.toLowerCase();
  if (BY_NAME[lower]) return BY_NAME[lower];
  const dot = lower.lastIndexOf('.');
  const ext = dot > 0 ? lower.slice(dot + 1) : '';
  return BY_EXT[ext] ?? DEFAULT;
}
