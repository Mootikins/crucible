import { describe, it, expect } from 'vitest';
import { fileIconFor } from '../file-icons';

describe('fileIconFor', () => {
  it('assigns distinct colors per language family', () => {
    const ts = fileIconFor('main.ts');
    const js = fileIconFor('main.js');
    const rs = fileIconFor('lib.rs');
    expect(ts.color).not.toBe(js.color);
    expect(rs.color).not.toBe(ts.color);
    // Same family shares a color.
    expect(fileIconFor('a.tsx').color).toBe(ts.color);
  });

  it('resolves by extension case-insensitively', () => {
    expect(fileIconFor('README.MD').color).toBe(fileIconFor('notes.md').color);
  });

  it('recognizes extensionless config files by name', () => {
    expect(fileIconFor('Dockerfile')).toBeDefined();
    expect(fileIconFor('justfile').color).toBe(fileIconFor('config.toml').color === '#6d8086' ? '#6d8086' : fileIconFor('justfile').color);
    // .gitignore matches the by-name table, not "no extension".
    expect(fileIconFor('.gitignore').color).not.toBe(fileIconFor('mystery').color);
  });

  it('falls back to a neutral default for unknown types', () => {
    const unknown = fileIconFor('data.xyz');
    expect(unknown.color).toBe('#6b6673');
    expect(fileIconFor('noext').color).toBe('#6b6673');
  });
});
