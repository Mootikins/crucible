/**
 * Shared button vocabulary. The old primary buttons were full-saturation
 * `bg-primary text-white` slabs — loud against a UI whose color language is
 * tinted chips (color/10 fills + color/40 hairlines). Rectangular action
 * buttons use the tinted treatment; ONLY the per-surface send CTA (chat
 * input, composer) stays solid primary.
 */

/** Tinted primary action (confirm, add, install…). */
export const btnPrimary =
  'px-3 py-1.5 rounded-md text-xs font-medium bg-primary/15 text-primary border border-primary/40 ' +
  'hover:bg-primary/25 hover:border-primary/60 transition-colors ' +
  'disabled:opacity-50 disabled:cursor-not-allowed';

/** Quiet neutral action beside a primary (cancel, dismiss…). */
export const btnNeutral =
  'px-3 py-1.5 rounded-md text-xs bg-surface-elevated text-shell-body border border-hairline ' +
  'hover:bg-hover-wash transition-colors disabled:opacity-50 disabled:cursor-not-allowed';

/**
 * The consequential half of a consent pair — the one that GRANTS something.
 *
 * `attention` rather than `primary`: primary is the friendly confirm used for
 * "add", "install", "save", and wearing it made Allow the reflex click of a
 * pair whose two halves cost wildly different amounts. Attention is the
 * theme's "this is waiting on your judgement" hue, which is what an Allow is.
 * Its partner is `btnNeutral`, deliberately quieter — see PermissionInteraction.
 */
export const btnConsent =
  'px-3 py-1.5 rounded-md text-xs font-medium bg-attention/15 text-attention border border-attention/50 ' +
  'hover:bg-attention/25 hover:border-attention/70 transition-colors ' +
  'disabled:opacity-50 disabled:cursor-not-allowed';
