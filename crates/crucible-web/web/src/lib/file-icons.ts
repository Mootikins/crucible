import type { Component, JSX } from 'solid-js';
import {
  ChartNetwork,
  File,
  FileText,
  FileCode,
  FileImage,
  FileArchive,
  FileLock,
  Film,
  Music,
  FileSpreadsheet,
  FileType,
  FileKey,
  Binary,
  Coffee,
  Gem,
  Parentheses,
  Hexagon,
  BookMarked,
  Braces,
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

// seti-ish palette
const BLUE = '#519aba';
const YELLOW = '#cbcb41';
const ORANGE = '#e37933';
const RUST = '#dea584';
const PURPLE = '#a074c4';
const GREEN = '#8dc149';
const GRAY = '#6d8086';
const PINK = '#cc6699';
const RED = '#cc3e44';
const TEAL = '#4bb1a7';
const MUTED = '#98939e';

// Shared metas keep the map terse.
const doc: FileIconMeta = { icon: FileText, color: BLUE };
const code = (color: string): FileIconMeta => ({ icon: FileCode, color });
const json: FileIconMeta = { icon: Braces, color: YELLOW };
const cfg = (color: string): FileIconMeta => ({ icon: Cog, color });
const style = (color: string): FileIconMeta => ({ icon: Palette, color });
const web = (color: string): FileIconMeta => ({ icon: Globe, color });
const img: FileIconMeta = { icon: FileImage, color: PURPLE };
const video: FileIconMeta = { icon: Film, color: PINK };
const audio: FileIconMeta = { icon: Music, color: TEAL };
const archive: FileIconMeta = { icon: FileArchive, color: MUTED };
const shell: FileIconMeta = { icon: Terminal, color: GREEN };
const data: FileIconMeta = { icon: Database, color: ORANGE };
const sheet: FileIconMeta = { icon: FileSpreadsheet, color: GREEN };
const key: FileIconMeta = { icon: FileKey, color: YELLOW };
const cert: FileIconMeta = { icon: FileLock, color: GRAY };
const font: FileIconMeta = { icon: FileType, color: RED };
const binary: FileIconMeta = { icon: Binary, color: GRAY };
const lisp: FileIconMeta = { icon: Parentheses, color: PURPLE };

const BY_EXT: Record<string, FileIconMeta> = {
  // --- prose / docs ---
  md: doc, markdown: doc, mdx: doc, mdc: doc,
  txt: { icon: FileText, color: MUTED },
  rst: doc, adoc: doc, asciidoc: doc, org: doc, tex: doc, bib: doc, rtf: doc,
  doc: doc, docx: doc, odt: doc, pdf: { icon: FileText, color: RED },
  epub: { icon: BookMarked, color: GREEN }, log: { icon: FileText, color: MUTED },
  license: doc, readme: doc,

  // --- languages ---
  ts: code(BLUE), mts: code(BLUE), cts: code(BLUE), 'd.ts': code(BLUE),
  tsx: code(BLUE),
  js: code(YELLOW), jsx: code(YELLOW), mjs: code(YELLOW), cjs: code(YELLOW),
  rs: code(RUST),
  py: code(BLUE), pyi: code(BLUE), pyw: code(BLUE),
  go: code('#00add8'),
  rb: { icon: Gem, color: RED }, erb: { icon: Gem, color: RED }, gemspec: { icon: Gem, color: RED },
  java: { icon: Coffee, color: ORANGE }, class: { icon: Coffee, color: ORANGE }, jar: { icon: Coffee, color: ORANGE },
  kt: code('#a97bff'), kts: code('#a97bff'),
  scala: code(RED), sbt: code(RED),
  c: code(BLUE), h: code(PURPLE),
  cpp: code(BLUE), cc: code(BLUE), cxx: code(BLUE), hpp: code(PURPLE), hh: code(PURPLE), hxx: code(PURPLE),
  cs: code(GREEN), csx: code(GREEN),
  php: code(PURPLE),
  swift: code(ORANGE),
  dart: code(TEAL),
  ex: code(PURPLE), exs: code(PURPLE), eex: code(PURPLE), heex: code(PURPLE),
  erl: code(RED), hrl: code(RED),
  clj: lisp, cljs: lisp, cljc: lisp, edn: lisp, lisp: lisp, el: lisp, scm: lisp, rkt: lisp,
  hs: code(PURPLE), lhs: code(PURPLE),
  ml: code(ORANGE), mli: code(ORANGE), fs: code(BLUE), fsx: code(BLUE), fsi: code(BLUE),
  nim: code(YELLOW), zig: code(ORANGE), v: code(BLUE), vala: code(PURPLE),
  sol: { icon: Hexagon, color: GRAY },
  r: code(BLUE), jl: code(PURPLE),
  pl: code(BLUE), pm: code(BLUE),
  lua: { icon: Moon, color: '#51a0cf' }, fnl: { icon: Moon, color: PURPLE },
  vim: code(GREEN),
  groovy: code(BLUE), gradle: code(TEAL),

  // --- data / structured ---
  json: json, jsonc: json, json5: json, geojson: json, ndjson: json,
  toml: cfg(GRAY), ini: cfg(GRAY), conf: cfg(GRAY), cfg: cfg(GRAY),
  properties: cfg(GRAY), editorconfig: cfg(GRAY), env: key,
  yaml: cfg(PINK), yml: cfg(PINK),
  xml: web(ORANGE), plist: cfg(GRAY),
  csv: sheet, tsv: sheet, xlsx: sheet, xls: sheet, ods: sheet, parquet: data, arrow: data,
  sql: data, db: { icon: Database, color: GRAY }, sqlite: { icon: Database, color: GRAY },
  sqlite3: { icon: Database, color: GRAY }, prisma: code(TEAL), graphql: web(PINK), gql: web(PINK),
  proto: code(BLUE),

  // --- web / markup / style ---
  html: web(ORANGE), htm: web(ORANGE), xhtml: web(ORANGE),
  vue: code(GREEN), svelte: code(ORANGE), astro: code(ORANGE),
  css: style(BLUE), scss: style(PINK), sass: style(PINK), less: style(BLUE), styl: style(GREEN),

  // --- knowledge ---
  // A .canvas is a JSON Canvas document, not an image — it used to map to the
  // image icon, which read as "picture" in every file tree.
  canvas: { icon: ChartNetwork, color: PURPLE },

  // --- images ---
  svg: { icon: FileImage, color: ORANGE },
  png: img, jpg: img, jpeg: img, gif: img, webp: img, ico: img, bmp: img,
  tiff: img, tif: img, avif: img, heic: img, psd: img, ai: img, xcf: img,

  // --- video / audio ---
  mp4: video, mov: video, webm: video, mkv: video, avi: video, m4v: video, flv: video,
  mp3: audio, wav: audio, flac: audio, ogg: audio, m4a: audio, aac: audio, opus: audio, mid: audio,

  // --- archives ---
  zip: archive, tar: archive, gz: archive, tgz: archive, xz: archive, bz2: archive,
  '7z': archive, rar: archive, zst: archive, lz4: archive,

  // --- shell / scripts ---
  sh: shell, bash: shell, zsh: shell, fish: shell, ksh: shell,
  ps1: shell, psm1: shell, bat: shell, cmd: shell, nu: shell,

  // --- keys / certs / secrets ---
  pem: cert, key: key, crt: cert, cert: cert, cer: cert, pub: key, asc: cert, gpg: cert, p12: cert,
  lock: { icon: FileLock, color: GRAY },

  // --- fonts ---
  ttf: font, otf: font, woff: font, woff2: font, eot: font,

  // --- notebooks / binaries ---
  ipynb: code(ORANGE),
  bin: binary, exe: binary, dll: binary, so: binary, dylib: binary,
  o: binary, a: binary, obj: binary, wasm: binary, class_: binary,
};

/** Files recognized by full name (extensionless or special). */
const BY_NAME: Record<string, FileIconMeta> = {
  dockerfile: cfg(BLUE),
  'docker-compose.yml': cfg(BLUE),
  'docker-compose.yaml': cfg(BLUE),
  makefile: cfg(GRAY),
  justfile: cfg(GRAY),
  'cmakelists.txt': cfg(GRAY),
  'cargo.toml': cfg(RUST),
  'cargo.lock': { icon: FileLock, color: RUST },
  'package.json': json,
  'package-lock.json': { icon: FileLock, color: RED },
  'tsconfig.json': cfg(BLUE),
  '.gitignore': cfg(ORANGE),
  '.gitattributes': cfg(ORANGE),
  '.dockerignore': cfg(BLUE),
  '.env': key,
  '.editorconfig': cfg(GRAY),
  '.npmrc': cfg(RED),
  '.prettierrc': cfg(BLUE),
  '.eslintrc': cfg(PURPLE),
  license: doc,
  'license.md': doc,
  readme: doc,
  'readme.md': doc,
};

const DEFAULT: FileIconMeta = { icon: File, color: '#6b6673' };

/** Resolve the colored icon for a filename. */
export function fileIconFor(filename: string): FileIconMeta {
  const lower = filename.toLowerCase();
  if (BY_NAME[lower]) return BY_NAME[lower];
  // Compound extensions first (e.g. `foo.d.ts`, `bar.tar.gz`).
  if (lower.endsWith('.d.ts')) return BY_EXT['d.ts'];
  if (lower.endsWith('.tar.gz') || lower.endsWith('.tar.bz2') || lower.endsWith('.tar.xz')) return BY_EXT.tar;
  const dot = lower.lastIndexOf('.');
  const ext = dot > 0 ? lower.slice(dot + 1) : '';
  return BY_EXT[ext] ?? DEFAULT;
}
