import { describe, it, expect } from 'vitest';
import { pluginVersionLabel } from '../plugin-version';

describe('pluginVersionLabel', () => {
  it('prefixes a declared version with v', () => {
    expect(pluginVersionLabel('1.2.3')).toBe('v1.2.3');
  });

  it('names the unknown state instead of rendering the null the daemon sent', () => {
    // The daemon reports null for a plugin it has discovered but not loaded.
    // Both settings surfaces put this string in a row, so neither may show
    // "vnull", a bare "v", or the old "0.0.0" placeholder.
    for (const unknown of [null, undefined, '']) {
      expect(pluginVersionLabel(unknown)).toBe('version unknown');
    }
  });
});
