import type { CreateSessionParams } from '@/lib/types';

/**
 * The runtime chip's built-in "run here" row.
 *
 * Not a `provider:target` spec and deliberately not published by anything:
 * running on this machine is what happens when no provider is asked, so it
 * cannot depend on a plugin being installed. Picking it says "not isolated"
 * out loud, which the daemon acts on differently from saying nothing.
 */
export const HOST_RUNTIME = 'host';

/** Registry names only: a nameless entry cannot be attached by name. */
export function kilnsForCreate(chosen: string, fallback: string | null): string[] {
  if (chosen === 'none') return [];
  const name = chosen || fallback;
  return name ? [name] : [];
}

export interface DraftChoices {
  /** Kiln registry name, '' for the default, 'none' for no kiln. */
  kiln: string;
  defaultKiln: string | null;
  workspace: string;
  /** ACP agent name, or '' for the internal agent. */
  agentName: string;
  /** Optional internal-agent card name, resolved by the daemon. */
  agentCard?: string;
  /** `provider:target`, or '' for untouched. */
  wsTarget: string;
  /** `provider:target`, `host`, or '' for untouched. */
  runtime: string;
}

/**
 * What the draft sends to `createSession`, from what the user chose.
 *
 * One copy, because the desktop composer and the phone's sheet both build it,
 * and the empty-value contract is easy to get wrong in a second place: an
 * UNTOUCHED runtime must send nothing, so the project's own setting decides,
 * while `host` must send `false`, which overrides it. A `false` that gets
 * dropped unsandboxes a session; an omission that becomes `false` sandboxes one
 * the user never asked to isolate.
 */
export function draftCreateParams(choices: DraftChoices): CreateSessionParams {
  const isAcp = choices.agentName !== '';
  return {
    kilns: kilnsForCreate(choices.kiln, choices.defaultKiln),
    workspace: choices.workspace || undefined,
    ...(isAcp ? { agent_type: 'acp' as const, agent_name: choices.agentName } : {}),
    ...(!isAcp && choices.agentCard?.trim() ? { agent_card: choices.agentCard.trim() } : {}),
    // The daemon resolves this to a path before it creates anything, so the
    // session is born in the right checkout.
    ...(choices.wsTarget ? { workspace_target: choices.wsTarget } : {}),
    ...runtimeParam(choices.runtime),
  };
}

function runtimeParam(spec: string): Partial<Pick<CreateSessionParams, 'isolation'>> {
  if (!spec) return {};
  if (spec === HOST_RUNTIME) return { isolation: false };
  const [plugin, ...rest] = spec.split(':');
  return { isolation: { plugin, target: rest.join(':') } };
}
