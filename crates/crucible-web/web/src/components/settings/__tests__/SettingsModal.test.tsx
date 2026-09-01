import { describe, it, expect } from 'vitest';
import { settingsSections, settingsGroups } from '../sections';

describe('the settings section registry', () => {
  it('builds without touching a partially-initialised module', () => {
    // A regression gate with real history. This table used to be a top-level
    // `const` array, and it imports its section components from
    // SettingsPanel.tsx, which imports the table back to render the stacked
    // tab. The array evaluated those bindings mid-initialisation and threw
    // `ReferenceError: Cannot access 'AppearanceSettingsSection' before
    // initialization` at boot — with every unit test green, because none of
    // them imported both modules in that order. Importing this module ALONE
    // and building the table is exactly the order that used to fail.
    expect(() => settingsSections()).not.toThrow();
    expect(settingsSections().length).toBeGreaterThan(0);
  });

  it('gives every section an id, a label and something to render', () => {
    for (const section of settingsSections()) {
      expect(section.id, 'id').toBeTruthy();
      expect(section.label, `${section.id} label`).toBeTruthy();
      expect(typeof section.render, `${section.id} render`).toBe('function');
      expect(typeof section.icon, `${section.id} icon`).toBe('function');
    }
  });

  it('keeps ids unique — the left list keys on them', () => {
    const ids = settingsSections().map((s) => s.id);
    expect(new Set(ids).size).toBe(ids.length);
  });

  it('groups sections contiguously, preserving declaration order', () => {
    const groups = settingsGroups(settingsSections());
    // Every section reachable through the grouping, exactly once and in order.
    const flat = groups.flatMap((g) => g.sections.map((s) => s.id));
    expect(flat).toEqual(settingsSections().map((s) => s.id));
    // And a group name never appears twice — a split group would print the
    // same heading in two places in the list.
    const names = groups.map((g) => g.group);
    expect(new Set(names).size).toBe(names.length);
  });

  it('offers the workspace reset, which has no other doorway', () => {
    expect(settingsSections().some((s) => s.id === 'workspace')).toBe(true);
  });

  it('gives a plugin it has never heard of its own section', () => {
    // The property, not the plugin: no branch on identity anywhere in the
    // registry, so a plugin shipped tomorrow gets a left-list entry for free.
    const sections = settingsSections({
      'never-heard-of-this-one': {
        type: 'group',
        name: 'Widget Factory',
        args: [{ key: 'shade', type: 'input', name: 'Shade' }],
      } as never,
    });
    const added = sections.find((s) => s.id === 'plugin:never-heard-of-this-one');
    expect(added, 'the plugin has a section').toBeTruthy();
    expect(added!.label).toBe('Widget Factory');
    expect(added!.group).toBe('Plugins');
    // Namespaced, so a plugin named after a built-in cannot collide with it.
    expect(sections.filter((s) => s.id === 'editor').length).toBe(1);
  });

  it('drops a plugin that declares an empty tree', () => {
    // An entry that opens onto nothing is worse than no entry.
    const sections = settingsSections({
      quiet: { type: 'group', name: 'Quiet', args: [] } as never,
    });
    expect(sections.some((s) => s.id === 'plugin:quiet')).toBe(false);
  });

  it('keeps the built-in sections when no plugin tree is available', () => {
    // A failed plugin fetch must not cost the user the app's OWN settings —
    // that is exactly when they most need to reach them.
    expect(settingsSections(undefined).map((s) => s.id)).toEqual(
      settingsSections({}).map((s) => s.id),
    );
    expect(settingsSections(undefined).length).toBeGreaterThan(0);
  });
});
