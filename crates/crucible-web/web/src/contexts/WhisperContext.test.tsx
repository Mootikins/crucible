import { describe, it, expect } from 'vitest';
import { mixToMono, resampleTo } from './WhisperContext';

// Whisper expects 16 kHz mono PCM. These exercise the REAL DSP helpers extracted
// from decodeAudioBlob. (The previous tests reimplemented the mix/resample math
// inline and asserted on their own arithmetic, so WhisperContext itself was never
// exercised — any regression in the real code passed green.)

describe('WhisperContext audio DSP', () => {
  describe('mixToMono', () => {
    it('returns the single channel unchanged for mono input', () => {
      const mono = new Float32Array([0.1, 0.2, 0.3, 0.4]);
      // Mono passes through untouched — same buffer, no copy/mix.
      expect(mixToMono([mono])).toBe(mono);
    });

    it('averages left and right for stereo input', () => {
      const left = new Float32Array([0.2, 0.4, 0.6, 0.8]);
      const right = new Float32Array([0.4, 0.6, 0.8, 1.0]);
      const mono = mixToMono([left, right]);
      expect(mono[0]).toBeCloseTo(0.3);
      expect(mono[1]).toBeCloseTo(0.5);
      expect(mono[2]).toBeCloseTo(0.7);
      expect(mono[3]).toBeCloseTo(0.9);
    });

    it('returns an empty buffer for no channels', () => {
      expect(mixToMono([]).length).toBe(0);
    });
  });

  describe('resampleTo', () => {
    it('returns the input unchanged when rates match', () => {
      const data = new Float32Array([1, 2, 3]);
      expect(resampleTo(data, 16000, 16000)).toBe(data);
    });

    it('downsamples 48kHz to 16kHz at a 3:1 ratio (nearest-neighbour)', () => {
      const src = new Float32Array(48);
      for (let i = 0; i < src.length; i++) src[i] = i;
      const out = resampleTo(src, 48000, 16000);
      expect(out.length).toBe(16);
      expect(out[0]).toBe(0);
      expect(out[1]).toBe(3);
      expect(out[2]).toBe(6);
    });

    it('upsamples 8kHz to 16kHz at a 1:2 ratio', () => {
      const src = new Float32Array([0, 1, 2, 3]);
      const out = resampleTo(src, 8000, 16000);
      expect(out.length).toBe(8);
      expect(out[0]).toBe(0);
      expect(out[1]).toBe(0);
      expect(out[2]).toBe(1);
    });
  });
});
