import { readFileSync, statSync } from 'node:fs';

/**
 * Parse a cargo dep-info file (the `<bin>.d` cargo writes next to a build
 * product) into the list of paths cargo named as inputs of that product.
 *
 * The format is a Makefile rule per line: `target: dep1 dep2 dep3 ...`, with
 * a space inside a path escaped as `\ `. This function reads every rule the
 * file holds and returns the union of their dependency lists, so a dep-info
 * file naming more than one output (rare, but not disallowed) is still read
 * in full.
 */
export function parseCargoDepFile(contents: string): string[] {
  const paths: string[] = [];
  for (const rawLine of contents.split('\n')) {
    const line = rawLine.trimEnd();
    if (!line) continue;
    const colon = line.indexOf(': ');
    if (colon === -1) continue;
    const rest = line.slice(colon + 2);
    if (!rest) continue;
    // Split on runs of whitespace that are not themselves escaped.
    for (const token of rest.split(/(?<!\\)\s+/)) {
      if (!token) continue;
      paths.push(token.replace(/\\(.)/g, '$1'));
    }
  }
  return paths;
}

/**
 * First cargo-tracked input newer than `binaryPath`, or null when the binary
 * is at least as new as everything cargo's own dep-info lists for it.
 *
 * This reads `depFilePath` — cargo's own record of what it compiled INTO the
 * binary — instead of walking `src/` by hand. A file cargo never linked in
 * (a `#[cfg(test)]` module, an integration test under a crate's `tests` dir) is
 * simply absent from that record, so editing one can never report the
 * product stale for a change that never touched it.
 *
 * `ignoredDirs` excludes inputs under given directories from the comparison.
 * `crucible-web` embeds `web/dist` at compile time (`rust-embed`), so cargo's
 * dep-info genuinely lists it as a build input — but the live tier's own
 * recipe builds `cru` and then `web/dist`, in that order, every time, and
 * serves the freshly built directory via `--static-dir` rather than the
 * embed. `web/dist` has its own freshness check (`assertDistFresh`) against
 * its real sources; without this exclusion the recipe's own ordering would
 * report the binary stale on every run.
 *
 * Throws if `depFilePath` does not exist: an absent dep-info means no build
 * ever produced `binaryPath` from this dependency tracking, which is a
 * refusal, not a pass.
 */
export function findStaleCargoInput(
  binaryPath: string,
  depFilePath: string,
  ignoredDirs: string[] = [],
): string | null {
  const binaryMtimeMs = statSync(binaryPath).mtimeMs;
  const inputs = parseCargoDepFile(readFileSync(depFilePath, 'utf-8'));
  const isIgnored = (input: string): boolean =>
    ignoredDirs.some((dir) => input === dir || input.startsWith(`${dir}/`));
  for (const input of inputs) {
    if (isIgnored(input)) continue;
    let inputMtimeMs: number;
    try {
      inputMtimeMs = statSync(input).mtimeMs;
    } catch {
      // An input cargo once recorded but that no longer exists (a rename, a
      // deleted source) is not evidence of staleness; the next real build
      // drops it from the list. Skip it rather than fail an unrelated run.
      continue;
    }
    if (inputMtimeMs > binaryMtimeMs) return input;
  }
  return null;
}
