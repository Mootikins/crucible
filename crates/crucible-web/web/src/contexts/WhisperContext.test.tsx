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

/**
 * A failure inside the provider has to reach the notification area.
 *
 * The provider kept its message in an `error` signal that only the button's
 * tooltip read. A model that fails to download, or a transcription server
 * that answers 503, left the user with a red circle for three seconds and no
 * words. The notification names the cause.
 */
import { afterEach, beforeEach, vi } from 'vitest';
import { render, cleanup } from '@solidjs/testing-library';
import { SettingsProvider } from './SettingsContext';
import { WhisperProvider, useWhisperSafe, type WhisperContextValue } from './WhisperContext';
import { SETTINGS_STORAGE_KEY, defaultSettings } from '@/lib/settings';
import { notificationActions } from '@/stores/notificationStore';

const serverTranscribe = vi.fn();
vi.mock('@/lib/transcription', () => ({
  createServerTranscriber: () => serverTranscribe,
}));
const pipeline = vi.fn();
vi.mock('@huggingface/transformers', () => ({
  pipeline: (...args: unknown[]) => pipeline(...args),
}));

function mount(provider: 'server' | 'local'): WhisperContextValue {
  localStorage.setItem(
    SETTINGS_STORAGE_KEY,
    JSON.stringify({
      ...defaultSettings,
      transcription: { ...defaultSettings.transcription, provider, serverUrl: 'http://stt.local' },
    }),
  );
  let ctx: WhisperContextValue | undefined;
  const Probe = () => {
    ctx = useWhisperSafe();
    return null;
  };
  render(() => (
    <SettingsProvider>
      <WhisperProvider>
        <Probe />
      </WhisperProvider>
    </SettingsProvider>
  ));
  return ctx!;
}

describe('WhisperProvider reports failures to the notification area', () => {
  let added: ReturnType<typeof vi.spyOn>;

  beforeEach(() => {
    localStorage.clear();
    added = vi.spyOn(notificationActions, 'addNotification');
  });

  afterEach(() => {
    added.mockRestore();
    cleanup();
    vi.clearAllMocks();
  });

  it('names the server’s answer when a transcription request fails', async () => {
    serverTranscribe.mockRejectedValue(new Error('Transcription failed: 503 Service Unavailable'));
    const ctx = mount('server');

    await expect(ctx.transcribe(new Blob(['audio']))).rejects.toThrow();

    expect(added).toHaveBeenCalledWith(
      'error',
      expect.stringContaining('Transcription failed: 503 Service Unavailable'),
    );
    // The server needs no model, so the provider stays usable.
    expect(ctx.status()).toBe('ready');
  });

  it('names the cause when the local speech model cannot load', async () => {
    pipeline.mockRejectedValue(new Error('fetch failed: model.onnx'));
    const ctx = mount('local');

    await expect(ctx.loadModel()).rejects.toThrow();

    expect(added).toHaveBeenCalledWith(
      'error',
      expect.stringContaining('fetch failed: model.onnx'),
    );
    expect(ctx.status()).toBe('error');
  });
});
