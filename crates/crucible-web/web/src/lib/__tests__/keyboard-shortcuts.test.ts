// src/lib/__tests__/keyboard-shortcuts.test.ts
import { describe, it, expect } from 'vitest';
import {
  matchShortcut,
  DEFAULT_SHORTCUTS,
  ShortcutAction,
} from '../keyboard-shortcuts';

describe('keyboard-shortcuts', () => {
  describe('matchShortcut', () => {
    it('matches Ctrl+W to closeActiveTab', () => {
      const event = {
        key: 'w',
        ctrlKey: true,
        shiftKey: false,
        altKey: false,
        metaKey: false,
      } as KeyboardEvent;

      const result = matchShortcut(event);
      expect(result).toBe('closeActiveTab');
    });

    it('matches Ctrl+Tab to nextTab', () => {
      const event = {
        key: 'Tab',
        ctrlKey: true,
        shiftKey: false,
        altKey: false,
        metaKey: false,
      } as KeyboardEvent;

      const result = matchShortcut(event);
      expect(result).toBe('nextTab');
    });

    it('matches Ctrl+Shift+N to newSession', () => {
      const event = {
        key: 'n',
        ctrlKey: true,
        shiftKey: true,
        altKey: false,
        metaKey: false,
      } as KeyboardEvent;

      const result = matchShortcut(event);
      expect(result).toBe('newSession');
    });

    it('matches Ctrl+P to openCommandPalette', () => {
      const event = {
        key: 'p',
        ctrlKey: true,
        shiftKey: false,
        altKey: false,
        metaKey: false,
      } as KeyboardEvent;

      const result = matchShortcut(event);
      expect(result).toBe('openCommandPalette');
    });

    it('matches Escape (no modifiers) to closeOverlay', () => {
      const event = {
        key: 'Escape',
        ctrlKey: false,
        shiftKey: false,
        altKey: false,
        metaKey: false,
      } as KeyboardEvent;

      const result = matchShortcut(event);
      expect(result).toBe('closeOverlay');
    });

    it('matches Shift+Tab to cycleMode', () => {
      const event = {
        key: 'Tab',
        ctrlKey: false,
        shiftKey: true,
        altKey: false,
        metaKey: false,
      } as KeyboardEvent;

      const result = matchShortcut(event);
      expect(result).toBe('cycleMode');
    });

    it('matches Meta+W (Mac Cmd+W) to closeActiveTab', () => {
      const event = {
        key: 'w',
        ctrlKey: false,
        shiftKey: false,
        altKey: false,
        metaKey: true,
      } as KeyboardEvent;

      const result = matchShortcut(event);
      expect(result).toBe('closeActiveTab');
    });

    it('returns null for unmatched combo (Ctrl+Z)', () => {
      const event = {
        key: 'z',
        ctrlKey: true,
        shiftKey: false,
        altKey: false,
        metaKey: false,
      } as KeyboardEvent;

      const result = matchShortcut(event);
      expect(result).toBeNull();
    });

    it('returns null when key matches but modifiers do not', () => {
      const event = {
        key: 'w',
        ctrlKey: false,
        shiftKey: false,
        altKey: false,
        metaKey: false,
      } as KeyboardEvent;

      const result = matchShortcut(event);
      expect(result).toBeNull();
    });

    it('accepts custom shortcuts array', () => {
      const customShortcuts: ShortcutAction[] = [
        {
          key: 'x',
          modifiers: ['ctrl'],
          action: 'customAction',
          description: 'Custom action',
        },
      ];

      const event = {
        key: 'x',
        ctrlKey: true,
        shiftKey: false,
        altKey: false,
        metaKey: false,
      } as KeyboardEvent;

      const result = matchShortcut(event, customShortcuts);
      expect(result).toBe('customAction');
    });
  });

  describe('DEFAULT_SHORTCUTS', () => {
    it('has no duplicate action values', () => {
      const actions = DEFAULT_SHORTCUTS.map((s) => s.action);
      const uniqueActions = new Set(actions);
      expect(uniqueActions.size).toBe(actions.length);
    });

    it('all shortcuts have required properties', () => {
      DEFAULT_SHORTCUTS.forEach((shortcut) => {
        expect(shortcut).toHaveProperty('key');
        expect(shortcut).toHaveProperty('modifiers');
        expect(shortcut).toHaveProperty('action');
        expect(shortcut).toHaveProperty('description');
        expect(typeof shortcut.key).toBe('string');
        expect(Array.isArray(shortcut.modifiers)).toBe(true);
        expect(typeof shortcut.action).toBe('string');
        expect(typeof shortcut.description).toBe('string');
      });
    });

    it('all modifiers are valid', () => {
      const validModifiers = ['ctrl', 'shift', 'alt', 'meta'];
      DEFAULT_SHORTCUTS.forEach((shortcut) => {
        shortcut.modifiers.forEach((modifier) => {
          expect(validModifiers).toContain(modifier);
        });
      });
    });

    it('contains expected shortcuts', () => {
      const actionSet = new Set(DEFAULT_SHORTCUTS.map((s) => s.action));
      expect(actionSet.has('closeActiveTab')).toBe(true);
      expect(actionSet.has('nextTab')).toBe(true);
      expect(actionSet.has('newSession')).toBe(true);
      expect(actionSet.has('openCommandPalette')).toBe(true);
      expect(actionSet.has('closeOverlay')).toBe(true);
      expect(actionSet.has('cycleMode')).toBe(true);
    });
  });
});
