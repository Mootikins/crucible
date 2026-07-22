import { describe, it, expect } from 'vitest';
import { findLinkingBlock, wikilinkTargetMatches } from '../backlink-context';

describe('wikilinkTargetMatches', () => {
  it('matches exact keys case-insensitively', () => {
    expect(wikilinkTargetMatches('Kilns', ['kilns'])).toBe(true);
  });
  it('matches path targets against stems and vice versa', () => {
    expect(wikilinkTargetMatches('Help/Concepts/Kilns', ['Kilns'])).toBe(true);
    expect(wikilinkTargetMatches('Kilns', ['Help/Concepts/Kilns'])).toBe(true);
  });
  it('ignores .md suffixes on keys', () => {
    expect(wikilinkTargetMatches('Kilns', ['Kilns.md'])).toBe(true);
  });
  it('rejects unrelated targets', () => {
    expect(wikilinkTargetMatches('Ovens', ['Kilns'])).toBe(false);
  });
});

describe('findLinkingBlock', () => {
  const doc = [
    '# Intro',
    '',
    'Some unrelated line with [[Other Note]].',
    '- See [[Help/Concepts/Kilns|the kiln docs]] for details.',
    'Later mention of [[Kilns]] again.',
  ].join('\n');

  it('finds the first line whose wikilink hits the target (alias + path forms)', () => {
    const block = findLinkingBlock(doc, ['Kilns']);
    expect(block).not.toBeNull();
    expect(block!.line).toBe(4);
    // List marker stripped from the snippet.
    expect(block!.snippet).toBe('See [[Help/Concepts/Kilns|the kiln docs]] for details.');
  });

  it('matches heading-anchored links', () => {
    const b = findLinkingBlock('x [[Kilns#Setup]] y', ['Kilns']);
    expect(b?.line).toBe(1);
  });

  it('returns null when nothing links to the target', () => {
    expect(findLinkingBlock(doc, ['Nonexistent'])).toBeNull();
  });

  it('clamps very long lines around the link', () => {
    const long = 'a'.repeat(300) + ' [[Kilns]] ' + 'b'.repeat(300);
    const b = findLinkingBlock(long, ['Kilns']);
    expect(b).not.toBeNull();
    expect(b!.snippet.length).toBeLessThan(220);
    expect(b!.snippet).toContain('[[Kilns]]');
  });
});
