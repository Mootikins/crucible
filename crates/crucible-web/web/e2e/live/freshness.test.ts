import { describe, expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import path from 'node:path';
import { findStaleCargoInput, parseCargoDepFile } from './freshness';

/**
 * These guard the live tier's own staleness gate (`assertBinaryFresh` in
 * `global-setup.ts`). The gate used to walk `crates/**\/*.rs` by hand, which
 * counted a `#[cfg(test)]`-only file as a build input even though cargo never
 * links it into the `cru` binary — a false "cru is older than ..." failure.
 * `findStaleCargoInput` reads cargo's own dep-info (`<bin>.d`) instead, so
 * only files cargo actually compiled in can report the binary stale.
 */

function touch(file: string, mtimeMs: number): void {
  const seconds = mtimeMs / 1000;
  utimesSync(file, seconds, seconds);
}

function withTmpDir(fn: (dir: string) => void): void {
  const dir = mkdtempSync(path.join(os.tmpdir(), 'freshness-test-'));
  try {
    fn(dir);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}

describe('parseCargoDepFile', () => {
  test('reads the dependency list off a single rule', () => {
    const paths = parseCargoDepFile('/t/target/debug/cru: /t/src/main.rs /t/src/lib.rs\n');
    expect(paths).toEqual(['/t/src/main.rs', '/t/src/lib.rs']);
  });

  test('unescapes a space inside a path', () => {
    const paths = parseCargoDepFile('/t/target/debug/cru: /t/docs/Getting\\ Started.md\n');
    expect(paths).toEqual(['/t/docs/Getting Started.md']);
  });

  test('unions the dependency lists of multiple rules', () => {
    const paths = parseCargoDepFile(
      '/t/target/debug/cru: /t/a.rs\n/t/target/debug/cru.d: /t/a.rs /t/b.rs\n',
    );
    expect(paths).toEqual(['/t/a.rs', '/t/a.rs', '/t/b.rs']);
  });

  test('returns nothing for a blank file', () => {
    expect(parseCargoDepFile('')).toEqual([]);
  });
});

describe('findStaleCargoInput', () => {
  test('a real cargo input newer than the binary is reported stale', () => {
    withTmpDir((dir) => {
      const bin = path.join(dir, 'cru');
      const real = path.join(dir, 'main.rs');
      const depFile = path.join(dir, 'cru.d');
      writeFileSync(bin, 'binary');
      writeFileSync(real, 'fn main() {}');
      writeFileSync(depFile, `${bin}: ${real}\n`);

      const base = Date.now();
      touch(bin, base);
      touch(real, base + 60_000); // newer: a real edit after the last build

      expect(findStaleCargoInput(bin, depFile)).toBe(real);
    });
  });

  test('a test-only file newer than the binary, but absent from the dep-info, does not count', () => {
    withTmpDir((dir) => {
      const bin = path.join(dir, 'cru');
      const real = path.join(dir, 'main.rs');
      // Stands in for a #[cfg(test)]-only module: cargo never links it into
      // the `cru` binary, so it never appears in cru.d, no matter its mtime.
      const testOnly = path.join(dir, 'bench.rs');
      const depFile = path.join(dir, 'cru.d');
      writeFileSync(bin, 'binary');
      writeFileSync(real, 'fn main() {}');
      writeFileSync(testOnly, '#[cfg(test)]\nmod bench {}');
      writeFileSync(depFile, `${bin}: ${real}\n`);

      const base = Date.now();
      touch(bin, base);
      touch(real, base - 60_000); // older: the binary already reflects it
      touch(testOnly, base + 60_000); // newer, but not a build input

      expect(findStaleCargoInput(bin, depFile)).toBeNull();
    });
  });

  test('an input the dep-info lists but that no longer exists is skipped, not a crash', () => {
    withTmpDir((dir) => {
      const bin = path.join(dir, 'cru');
      const gone = path.join(dir, 'renamed-away.rs');
      const depFile = path.join(dir, 'cru.d');
      writeFileSync(bin, 'binary');
      writeFileSync(depFile, `${bin}: ${gone}\n`);

      expect(findStaleCargoInput(bin, depFile)).toBeNull();
    });
  });

  test('an input under an ignored directory does not count, even if newer', () => {
    withTmpDir((dir) => {
      const bin = path.join(dir, 'cru');
      const distDir = path.join(dir, 'dist');
      const embedded = path.join(distDir, 'index.html');
      const depFile = path.join(dir, 'cru.d');
      writeFileSync(bin, 'binary');
      mkdirSync(distDir);
      writeFileSync(embedded, '<html></html>');
      writeFileSync(depFile, `${bin}: ${distDir} ${embedded}\n`);

      const base = Date.now();
      touch(bin, base);
      touch(embedded, base + 60_000); // newer, but owned by a separate check

      expect(findStaleCargoInput(bin, depFile, [distDir])).toBeNull();
    });
  });

  test('throws when the dep-info file is missing', () => {
    withTmpDir((dir) => {
      const bin = path.join(dir, 'cru');
      writeFileSync(bin, 'binary');
      expect(() => findStaleCargoInput(bin, path.join(dir, 'cru.d'))).toThrow();
    });
  });
});
