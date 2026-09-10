/**
 * An argument form, derived from a command's declared parameters.
 *
 * A plugin declares `params` on a command; `signature.rs` turns each declared
 * type into JSON Schema, and `commands_json` ships that as `parameters`. This
 * module reads that schema and answers two questions a dialog needs: **which
 * control does each parameter get**, and **what does a filled-in form become
 * on the wire**. Nothing here knows a command's name, so a command written
 * tomorrow gets a form today.
 *
 * ## The control set is exactly what the schema can say
 *
 * `LuaType::to_json_schema` emits six shapes and no others, so there are five
 * controls and no others:
 *
 * | Declared      | Schema                       | Control    |
 * |---------------|------------------------------|------------|
 * | `string`      | `{type: "string"}`           | `text`     |
 * | `number`      | `{type: "number"}`           | `number`   |
 * | `boolean`     | `{type: "boolean"}`          | `checkbox` |
 * | `string[]`    | `{type: "array", items}`     | `lines`    |
 * | `{ a: T }`, `table<K,V>`, `T\|U`, `any`, a name | object, `anyOf`, `{}` | `json` |
 *
 * `json` is the honest answer rather than a fallback: a person editing a
 * `table<string, number>` is editing a table, and inventing a nested form for
 * a shape whose depth the schema does not bound would be guessing.
 *
 * ## What the declaration cannot say, and it matters
 *
 * There is **no enum**. The Luau type grammar has no string-literal type, so
 * `kanban_move`'s `to` — whose whole domain is the four column names that
 * board happens to have — declares `string` and gets a free-text box. That is
 * the first parameter anyone would want a dropdown for, and no amount of work
 * in this file can produce one. There is also no default, no range, and no
 * format: a `path` is a `string`, so a note picker is likewise unreachable.
 * Widening the type vocabulary is what would fix these; a richer dialog cannot.
 */

/** The control a parameter is drawn with. See the table in the module docs. */
export type CommandFieldControl = 'text' | 'number' | 'checkbox' | 'lines' | 'json';

/** One parameter, ready to draw. */
export interface CommandField {
  name: string;
  control: CommandFieldControl;
  required: boolean;
  /** The `desc` the plugin declared, or `''`. */
  description: string;
  /** The declared type, for a person: `string`, `number[]`, `object`. */
  typeLabel: string;
}

/** What a drawn form holds: a string per box, a boolean per checkbox. */
export type CommandFormValues = Record<string, string | boolean | undefined>;

/** The wire arguments, plus a message per field that could not be read. */
export interface CommandArgs {
  args: Record<string, unknown>;
  errors: Record<string, string>;
}

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === 'object' && value !== null && !Array.isArray(value);
}

function controlFor(property: Record<string, unknown>): CommandFieldControl {
  switch (property.type) {
    case 'string':
      return 'text';
    case 'number':
    case 'integer':
      return 'number';
    case 'boolean':
      return 'checkbox';
    case 'array':
      // A list of scalars is a textarea, one entry per line. A list of tables
      // is a table, so it goes to `json` with everything else shapeless.
      return scalarType(property.items) ? 'lines' : 'json';
    default:
      return 'json';
  }
}

/** The item type of an array, if it is one a line of text can hold. */
function scalarType(items: unknown): 'string' | 'number' | null {
  if (!isRecord(items)) return null;
  if (items.type === 'string') return 'string';
  if (items.type === 'number' || items.type === 'integer') return 'number';
  return null;
}

function typeLabelFor(property: Record<string, unknown>): string {
  if (typeof property.type === 'string') {
    if (property.type === 'array') {
      const item = isRecord(property.items) ? property.items.type : undefined;
      return typeof item === 'string' ? `${item}[]` : 'array';
    }
    return property.type;
  }
  if (Array.isArray(property.anyOf)) return 'one of several';
  return 'any';
}

/**
 * The fields a command's declared `parameters` describe.
 *
 * Anything that is not an object schema — a command that declared nothing, a
 * daemon too old to send the field, a shape this code has not met — answers
 * with no fields. A command with no arguments is a legitimate command, and it
 * must still get a dialog with a Run button rather than an error.
 */
export function commandFields(parameters: unknown): CommandField[] {
  if (!isRecord(parameters)) return [];
  const properties = parameters.properties;
  if (!isRecord(properties)) return [];
  const required = Array.isArray(parameters.required)
    ? parameters.required.filter((name): name is string => typeof name === 'string')
    : [];

  return Object.entries(properties).map(([name, raw]) => {
    const property = isRecord(raw) ? raw : {};
    return {
      name,
      control: controlFor(property),
      required: required.includes(name),
      description: typeof property.description === 'string' ? property.description : '',
      typeLabel: typeLabelFor(property),
    };
  });
}

/**
 * Turn a filled-in form into the arguments to send, or into per-field errors.
 *
 * **A blank optional field is omitted, not sent as `""`.** A plugin reads
 * `args.depth` and falls back to its own default when it is nil; `""` is not
 * nil, so sending the empty box would silently override every default the
 * plugin has. A blank *required* field is an error instead — the schema says
 * the command cannot run without it.
 */
export function commandArgs(fields: CommandField[], values: CommandFormValues): CommandArgs {
  const args: Record<string, unknown> = {};
  const errors: Record<string, string> = {};

  for (const field of fields) {
    const raw = values[field.name];

    if (field.control === 'checkbox') {
      // A checkbox always has an answer once it is drawn; an untouched one is
      // a real `false`, which is different from "the plugin decides".
      if (raw === undefined) {
        if (field.required) errors[field.name] = 'required';
        continue;
      }
      args[field.name] = raw === true;
      continue;
    }

    const text = typeof raw === 'string' ? raw.trim() : '';
    if (text === '') {
      if (field.required) errors[field.name] = 'required';
      continue;
    }

    switch (field.control) {
      case 'text':
        args[field.name] = text;
        break;
      case 'number': {
        const parsed = Number(text);
        if (!Number.isFinite(parsed)) {
          errors[field.name] = `not a number: ${text}`;
        } else {
          args[field.name] = parsed;
        }
        break;
      }
      case 'lines': {
        const entries = text
          .split('\n')
          .map((line) => line.trim())
          .filter((line) => line !== '');
        args[field.name] = field.typeLabel.startsWith('number') ? entries.map(Number) : entries;
        break;
      }
      case 'json': {
        try {
          args[field.name] = JSON.parse(text);
        } catch (error) {
          errors[field.name] = error instanceof Error ? error.message : 'not valid JSON';
        }
        break;
      }
    }
  }

  return { args, errors };
}
