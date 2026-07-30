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

/** Tinted destructive action (deny, reject, delete…). Symmetric to btnPrimary
 *  in shape so the pair reads as one design vocabulary, not primary slab vs.
 *  bespoke red button. */
export const btnDanger =
  'px-3 py-1.5 rounded-md text-xs font-medium bg-error/15 text-error border border-error/40 ' +
  'hover:bg-error/25 hover:border-error/60 transition-colors ' +
  'disabled:opacity-50 disabled:cursor-not-allowed';
