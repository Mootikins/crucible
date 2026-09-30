/**
 * How the tree names a file. A note (`.md`) shows its name without the
 * extension. Any other file shows its name without the extension too, and
 * the extension as a label, as Obsidian's file explorer does. The real app
 * reads the file names from `useListDir`.
 */
import type { Component } from 'solid-js';
import { File, FileImage, FileJson, FileText, LayoutDashboard } from 'lucide-solid';

/** Icons: an icon on every file. Extensions: no icon, and a label on each file that is not a note. */
export type FileLabels = 'icons' | 'extensions';

export interface FileLabel {
  title: string;
  /** The extension in capitals, for a file that is not a note. */
  ext?: string;
}

export function fileLabel(name: string): FileLabel {
  const dot = name.lastIndexOf('.');
  // A name that starts with a dot, or has none, has no extension.
  if (dot <= 0) return { title: name };
  const ext = name.slice(dot + 1);
  if (ext.toLowerCase() === 'md') return { title: name.slice(0, dot) };
  return { title: name.slice(0, dot), ext: ext.toUpperCase() };
}

const ICONS: Record<string, Component<{ class?: string }>> = {
  CANVAS: LayoutDashboard,
  PNG: FileImage,
  JPG: FileImage,
  SVG: FileImage,
  JSON: FileJson,
};

/** The icon of a file in the Icons mode: a note, a canvas, an image, data, or any other file. */
export const fileIcon = (label: FileLabel): Component<{ class?: string }> =>
  label.ext ? ICONS[label.ext] ?? File : FileText;
