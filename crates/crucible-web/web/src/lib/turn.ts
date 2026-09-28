/**
 * Small pure helpers of the chat transcript view.
 *
 * The daemon mints every transcript id (`crates/crucible-core/src/transcript/`),
 * so no helper here derives one. They make no request, and keeping them here
 * lets the transcript layer import nothing from the api module.
 */

/**
 * The name in the `origin` of a `user_message` when its kind is `kind`:
 * `plugin` for a plugin turn, `relay` for a person's words that a plugin
 * relayed. An absent origin and the kind `user` mean a person here.
 */
export function originName(origin: unknown, kind: 'plugin' | 'relay'): string | undefined {
  const o = origin as { kind?: unknown; name?: unknown } | null | undefined;
  return o?.kind === kind && typeof o.name === 'string' ? o.name : undefined;
}

/**
 * Estimated token count for a thinking block's summary line — the same unit
 * the TUI's reasoning summary uses. The `~` in the rendered label carries the
 * approximation: no provider reports per-thinking-block usage on any wire
 * this page consumes (usage arrives once per completion, covering the whole
 * turn), so the estimate is the honest ceiling.
 */
export function estimateThinkingTokens(content: string): number {
  return Math.ceil([...content].length / 4);
}

/** A client-minted id for an optimistic message, unique within a page. */
export function generateMessageId(): string {
  return `msg_${Date.now()}_${Math.random().toString(36).substring(2, 9)}`;
}
