/**
 * How a plugin row states its version.
 *
 * A plugin the daemon has discovered but not loaded has no version: the
 * version lives in the plugin's spec table, and only a load reads that
 * table. The daemon reports `null` for it — it used to report a synthesized
 * `"0.0.0"`, which read as a real release.
 *
 * `v${null}` renders "vnull" and a bare `v` renders a dangling letter, so the
 * unknown state gets words instead of a version-shaped string.
 */
export function pluginVersionLabel(version: string | null | undefined): string {
  return version ? `v${version}` : 'version unknown';
}
