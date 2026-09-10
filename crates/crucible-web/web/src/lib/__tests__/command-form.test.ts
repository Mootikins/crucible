import { describe, it, expect } from 'vitest';
import { commandFields, commandArgs, type CommandField } from '../command-form';

/**
 * The whole point of this module is that it has never seen the command.
 *
 * So every schema below is invented here rather than taken from a shipped
 * plugin: if these tests passed only for `graph_neighborhood` and
 * `kanban_move`, the dialog would be a hand-written form with extra steps.
 * `signature.rs::to_json_schema` is what decides the shapes that can arrive,
 * and there is one case here per branch of it.
 */

/** What `to_input_schema` emits for `{ name = "x", type = T }`. */
function schema(properties: Record<string, unknown>, required: string[] = []) {
  return { type: 'object', properties, required };
}

describe('commandFields', () => {
  it('picks a control per declared type', () => {
    const fields = commandFields(
      schema(
        {
          path: { type: 'string', description: 'Kiln-relative note path' },
          depth: { type: 'number', description: 'Hops' },
          recurse: { type: 'boolean', description: '' },
          tags: { type: 'array', items: { type: 'string' }, description: '' },
          filter: { type: 'object', properties: {}, required: [], description: '' },
          whatever: { description: 'declared any' },
        },
        ['path'],
      ),
    );

    expect(fields.map((f) => [f.name, f.control])).toEqual([
      ['path', 'text'],
      ['depth', 'number'],
      ['recurse', 'checkbox'],
      ['tags', 'lines'],
      ['filter', 'json'],
      ['whatever', 'json'],
    ]);
  });

  it('carries required, description and a readable type label', () => {
    const fields = commandFields(
      schema({ path: { type: 'string', description: 'Where to look' } }, ['path']),
    );

    expect(fields[0]).toMatchObject({
      name: 'path',
      required: true,
      description: 'Where to look',
      typeLabel: 'string',
    });
  });

  it('labels an array by its item type', () => {
    const fields = commandFields(schema({ tags: { type: 'array', items: { type: 'number' } } }));
    expect(fields[0].typeLabel).toBe('number[]');
  });

  /**
   * A command with no declared parameters is the majority of what ships today.
   * It must produce a dialog with no fields, not an exception and not a
   * refusal to offer the command at all.
   */
  it('answers with no fields for a command that declared none', () => {
    expect(commandFields(schema({}))).toEqual([]);
    expect(commandFields(undefined)).toEqual([]);
    expect(commandFields(null)).toEqual([]);
    expect(commandFields('nonsense')).toEqual([]);
  });
});

describe('commandArgs', () => {
  const fields = (): CommandField[] =>
    commandFields(
      schema(
        {
          path: { type: 'string' },
          depth: { type: 'number' },
          recurse: { type: 'boolean' },
          tags: { type: 'array', items: { type: 'string' } },
          filter: { type: 'object' },
        },
        ['path'],
      ),
    );

  it('coerces each control to the type the schema declared', () => {
    const { args, errors } = commandArgs(fields(), {
      path: 'Meta/Oil.md',
      depth: '3',
      recurse: true,
      tags: 'alpha\nbeta',
      filter: '{"status":"todo"}',
    });

    expect(errors).toEqual({});
    expect(args).toEqual({
      path: 'Meta/Oil.md',
      depth: 3,
      recurse: true,
      tags: ['alpha', 'beta'],
      filter: { status: 'todo' },
    });
  });

  /**
   * An optional parameter left blank must be ABSENT, not `""`.
   *
   * A plugin reads `args.depth` and falls back to its own default when it is
   * nil; an empty string is not nil, and `clamp("")` is a different answer
   * from `clamp(nil)`. Sending the empty box is how a generated dialog
   * silently overrides every default a plugin has.
   */
  it('omits an optional field the person left blank', () => {
    const { args, errors } = commandArgs(fields(), { path: 'Meta/Oil.md' });

    expect(errors).toEqual({});
    expect(args).toEqual({ path: 'Meta/Oil.md' });
    expect(Object.keys(args)).not.toContain('depth');
  });

  it('refuses a required field left blank', () => {
    const { errors } = commandArgs(fields(), { path: '  ' });
    expect(errors.path).toMatch(/required/i);
  });

  it('refuses a number that is not one, naming the field', () => {
    const { errors } = commandArgs(fields(), { path: 'a.md', depth: 'deep' });
    expect(errors.depth).toMatch(/number/i);
  });

  it('refuses malformed JSON rather than sending the text', () => {
    const { args, errors } = commandArgs(fields(), { path: 'a.md', filter: '{oops' });
    expect(errors.filter).toBeTruthy();
    expect(args).not.toHaveProperty('filter');
  });

  it('drops blank lines from a list rather than sending empty entries', () => {
    const { args } = commandArgs(fields(), { path: 'a.md', tags: 'alpha\n\n  \nbeta\n' });
    expect(args.tags).toEqual(['alpha', 'beta']);
  });

  /** An unchecked box is a real `false`, not an omission. */
  it('sends a boolean either way once it has a value', () => {
    const { args } = commandArgs(fields(), { path: 'a.md', recurse: false });
    expect(args.recurse).toBe(false);
  });
});
