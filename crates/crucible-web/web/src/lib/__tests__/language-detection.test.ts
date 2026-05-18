import { describe, it, expect } from 'vitest';
import { languageFromFileName } from '../language-detection';

describe('languageFromFileName', () => {
  it.each([
    ['foo.rs', 'rust'],
    ['foo.ts', 'typescript'],
    ['foo.tsx', 'tsx'],
    ['foo.js', 'javascript'],
    ['foo.jsx', 'jsx'],
    ['foo.py', 'python'],
    ['foo.go', 'go'],
    ['foo.json', 'json'],
    ['foo.yaml', 'yaml'],
    ['foo.yml', 'yaml'],
    ['foo.toml', 'toml'],
    ['foo.md', 'markdown'],
    ['foo.html', 'html'],
    ['foo.css', 'css'],
    ['foo.sh', 'sh'],
    ['foo.bash', 'bash'],
    ['foo.sql', 'sql'],
    ['nested/path/to/foo.rs', 'rust'],
    ['Cargo.toml', 'toml'],
  ])('maps %s → %s', (fileName, expected) => {
    expect(languageFromFileName(fileName)).toBe(expected);
  });

  it('returns "text" for unknown extensions', () => {
    expect(languageFromFileName('foo.xyz')).toBe('text');
  });

  it('returns "text" for files with no extension', () => {
    expect(languageFromFileName('Makefile')).toBe('text');
  });

  it('returns "text" for undefined input', () => {
    expect(languageFromFileName(undefined)).toBe('text');
  });
});
