/**
 * WCAG 2.1 relative luminance and contrast ratio.
 *
 * The token layer in `index.css` is the only place a colour is written, so the
 * accessibility floor is proved by PARSING those tokens and computing, never by
 * a grep for a hex string — a source-text gate passes whether or not the value
 * behind it is legible.
 *
 * Reference: WCAG 2.1 §1.4.3 (contrast, minimum) and the relative-luminance
 * definition in the same specification.
 */

/** An sRGB colour, each channel 0-255. */
export interface Rgb {
  r: number;
  g: number;
  b: number;
}

/** Parse `#rgb`, `#rrggbb` or `#rrggbbaa` (alpha is dropped). Returns null on
 *  anything else — a `var()` reference, an `rgba()` wash, a keyword. */
export function parseHex(value: string): Rgb | null {
  const hex = value.trim().replace(/^#/, '');
  if (!/^[0-9a-fA-F]+$/.test(hex)) return null;
  if (hex.length === 3) {
    return {
      r: parseInt(hex[0] + hex[0], 16),
      g: parseInt(hex[1] + hex[1], 16),
      b: parseInt(hex[2] + hex[2], 16),
    };
  }
  if (hex.length === 6 || hex.length === 8) {
    return {
      r: parseInt(hex.slice(0, 2), 16),
      g: parseInt(hex.slice(2, 4), 16),
      b: parseInt(hex.slice(4, 6), 16),
    };
  }
  return null;
}

function channelLuminance(value255: number): number {
  const c = value255 / 255;
  return c <= 0.04045 ? c / 12.92 : Math.pow((c + 0.055) / 1.055, 2.4);
}

/** WCAG relative luminance, 0 (black) to 1 (white). */
export function relativeLuminance(color: Rgb): number {
  return (
    0.2126 * channelLuminance(color.r) +
    0.7152 * channelLuminance(color.g) +
    0.0722 * channelLuminance(color.b)
  );
}

/** WCAG contrast ratio, 1 to 21. Order of the two colours does not matter. */
export function contrastRatio(a: Rgb, b: Rgb): number {
  const la = relativeLuminance(a);
  const lb = relativeLuminance(b);
  const lighter = Math.max(la, lb);
  const darker = Math.min(la, lb);
  return (lighter + 0.05) / (darker + 0.05);
}

/** Contrast ratio of two hex strings. Throws if either does not parse, so a
 *  caller cannot silently score an unparsed colour as passing. */
export function contrastRatioHex(a: string, b: string): number {
  const ca = parseHex(a);
  const cb = parseHex(b);
  if (!ca) throw new Error(`not a hex colour: ${a}`);
  if (!cb) throw new Error(`not a hex colour: ${b}`);
  return contrastRatio(ca, cb);
}
