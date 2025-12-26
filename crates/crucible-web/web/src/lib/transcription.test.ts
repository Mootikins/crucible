// src/lib/transcription.test.ts
import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createServerTranscriber } from './transcription';

describe('createServerTranscriber', () => {
  const mockFetch = vi.fn();

  beforeEach(() => {
    vi.stubGlobal('fetch', mockFetch);
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('sends audio to OpenAI-compatible endpoint', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ text: 'hello world' }),
    });

    const transcribe = createServerTranscriber({
      url: 'https://llama.krohnos.io',
      model: 'whisper-1',
      language: 'auto',
    });

    const blob = new Blob(['fake audio'], { type: 'audio/webm' });
    const result = await transcribe(blob);

    expect(result).toBe('hello world');
    expect(mockFetch).toHaveBeenCalledWith(
      'https://llama.krohnos.io/v1/audio/transcriptions',
      expect.objectContaining({ method: 'POST' })
    );
  });

  it('includes model in form data', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ text: 'test' }),
    });

    const transcribe = createServerTranscriber({
      url: 'https://example.com',
      model: 'whisper-large-v3',
      language: 'auto',
    });

    await transcribe(new Blob(['audio']));

    const formData = mockFetch.mock.calls[0][1].body as FormData;
    expect(formData.get('model')).toBe('whisper-large-v3');
  });

  it('includes language when not auto', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ text: 'test' }),
    });

    const transcribe = createServerTranscriber({
      url: 'https://example.com',
      model: 'whisper-1',
      language: 'en',
    });

    await transcribe(new Blob(['audio']));

    const formData = mockFetch.mock.calls[0][1].body as FormData;
    expect(formData.get('language')).toBe('en');
  });

  it('omits language when set to auto', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: true,
      json: () => Promise.resolve({ text: 'test' }),
    });

    const transcribe = createServerTranscriber({
      url: 'https://example.com',
      model: 'whisper-1',
      language: 'auto',
    });

    await transcribe(new Blob(['audio']));

    const formData = mockFetch.mock.calls[0][1].body as FormData;
    expect(formData.get('language')).toBeNull();
  });

  it('throws on HTTP error', async () => {
    mockFetch.mockResolvedValueOnce({
      ok: false,
      status: 500,
      statusText: 'Internal Server Error',
    });

    const transcribe = createServerTranscriber({
      url: 'https://example.com',
      model: 'whisper-1',
      language: 'auto',
    });

    await expect(transcribe(new Blob(['audio']))).rejects.toThrow('Transcription failed: 500');
  });

  it('throws on network error', async () => {
    mockFetch.mockRejectedValueOnce(new Error('Network error'));

    const transcribe = createServerTranscriber({
      url: 'https://example.com',
      model: 'whisper-1',
      language: 'auto',
    });

    await expect(transcribe(new Blob(['audio']))).rejects.toThrow('Network error');
  });
});
