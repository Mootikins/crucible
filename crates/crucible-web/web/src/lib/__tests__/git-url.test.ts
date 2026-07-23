import { describe, it, expect } from 'vitest';
import { isGitRepoUrl } from '@/lib/api';

describe('isGitRepoUrl', () => {
  it('accepts https, ssh, and owner/repo shorthand', () => {
    expect(isGitRepoUrl('https://github.com/o/r')).toBe(true);
    expect(isGitRepoUrl('https://github.com/o/r.git')).toBe(true);
    expect(isGitRepoUrl('git@github.com:o/r.git')).toBe(true);
    expect(isGitRepoUrl('anthropics/claude-code')).toBe(true);
  });

  it('rejects local paths and non-repo strings', () => {
    expect(isGitRepoUrl('/home/moot/crucible')).toBe(false);
    expect(isGitRepoUrl('~/src/thing')).toBe(false);
    expect(isGitRepoUrl('./relative/dir')).toBe(false);
    expect(isGitRepoUrl('just-a-name')).toBe(false);
    expect(isGitRepoUrl('a/b/c')).toBe(false);
    expect(isGitRepoUrl('has spaces/repo')).toBe(false);
    expect(isGitRepoUrl('')).toBe(false);
  });
});
