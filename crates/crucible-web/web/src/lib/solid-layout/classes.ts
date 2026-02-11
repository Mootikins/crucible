/**
 * CSS class name helpers for SolidJS layout components.
 * All classes go through these helpers to support `classNameMapper`.
 * Re-exports CLASSES enum — no new CSS rules needed.
 */

export { CLASSES } from "../flexlayout/core/Types";

export type ClassNameMapper = (defaultClassName: string) => string;

export function cn(
  ...classes: (string | undefined | null | false)[]
): string {
  return classes.filter(Boolean).join(" ");
}

export function mapClass(
  defaultClass: string,
  mapper?: ClassNameMapper,
): string {
  return mapper ? mapper(defaultClass) : defaultClass;
}

export function buildClassName(
  base: string,
  modifiers: Record<string, boolean>,
  mapper?: ClassNameMapper,
): string {
  const parts: string[] = [mapClass(base, mapper)];

  for (const [cls, active] of Object.entries(modifiers)) {
    if (active) {
      parts.push(mapClass(cls, mapper));
    }
  }

  return parts.join(" ");
}
