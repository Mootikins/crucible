/**
 * How each tool reads in a transcript line: the verb when it is done, the
 * verb while it waits, its icon, and the noun that a folded group counts.
 */
import type { Component } from 'solid-js';
import { Eye, Pencil, Search } from 'lucide-solid';

export interface ToolWords {
  past: string;
  now?: string;
  icon: Component<{ class?: string }>;
  noun?: string;
}

export const TOOL: Record<string, ToolWords> = {
  read_note: { past: 'Read', icon: Eye, noun: 'read a note' },
  search_notes: { past: 'Searched notes for', icon: Search, noun: 'searched notes' },
  grep: { past: 'Searched code for', icon: Search, noun: 'searched code' },
  write_file: { past: 'Edited', now: 'Edit', icon: Pencil },
  write_note: { past: 'Edited', now: 'Edit', icon: Pencil },
};

export const toolWords = (name: string): ToolWords => TOOL[name] ?? { past: name, icon: Pencil };
