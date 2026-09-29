import type { InteractionOf } from './types';

/** A permission request, in the typed shape `InteractionOf<'permission'>` gives. */
export type PermRequest = InteractionOf<'permission'>;

/**
 * The four action kinds a permission request can name.
 *
 * Reads `request.action.type` — a tagged union — instead of the flat
 * `action_type` string the SSE reducer used to inject. The daemon's own
 * `PermRequest` never had that field; the web server's `normalize_interaction`
 * added it, and this module is what replaces that normalization now that the
 * browser reads the typed, kind-tagged contract directly.
 */
export function permActionType(request: PermRequest): 'bash' | 'read' | 'write' | 'tool' {
  return request.action.type;
}

/**
 * The tokens of the action, for display: a bash command's words, or a
 * read/write's path segments. Empty for a tool call, which names itself
 * through `permToolName` instead.
 */
export function permTokens(request: PermRequest): string[] {
  switch (request.action.type) {
    case 'bash':
      return request.action.tokens;
    case 'read':
    case 'write':
      return request.action.segments;
    case 'tool':
      return [];
  }
}

/** The tool's name, for a `tool` action only. */
export function permToolName(request: PermRequest): string | undefined {
  return request.action.type === 'tool' ? request.action.name : undefined;
}

/** The tool's raw arguments, for a `tool` action only. */
export function permToolArgs(request: PermRequest): unknown {
  return request.action.type === 'tool' ? request.action.args : undefined;
}
