/**
 * The slash-command values of the web client, without any request.
 *
 * The daemon owns each session's command catalog. The composer runs a
 * built-in command on the command route and sends every other `/name` as a
 * message, which the daemon routes. This module imports no API client, so a
 * component can use it.
 */
import type { components } from './api-schema';

type Schemas = components['schemas'];

/** One entry of a session's command catalog. */
export type SessionCommand = Schemas['SessionCommand'];

/** A command that the web client runs itself. */
export type BuiltinCommand = Schemas['BuiltinCommand'];

/**
 * The built-in commands. A `Record` over the generated union, so a built-in
 * command added in Rust does not compile here until the composer sends it to
 * the command route.
 */
const BUILTIN_COMMANDS: Record<BuiltinCommand, true> = {
  help: true,
  clear: true,
  model: true,
  mode: true,
  undo: true,
  resume: true,
  export: true,
  search: true,
};

/** Whether `/name` is a built-in command, which the command route runs. */
export function isBuiltinCommand(name: string): name is BuiltinCommand {
  return Object.hasOwn(BUILTIN_COMMANDS, name);
}

/** A command result as text: a string as it is, other JSON pretty-printed. */
export function commandResultText(result: unknown): string {
  if (typeof result === 'string') return result;
  if (result === null || result === undefined) return '';
  return JSON.stringify(result, null, 2);
}
