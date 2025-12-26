// src/lib/transcription.ts

/** Configuration for server-based transcription */
export interface ServerTranscriptionConfig {
  /** Base URL of the OpenAI-compatible transcription endpoint */
  url: string;
  /** Model name to use for transcription */
  model: string;
  /** Language code or 'auto' for automatic detection */
  language: string;
}

/**
 * Creates a transcription function that sends audio to an OpenAI-compatible endpoint.
 *
 * @param config - Configuration for the transcription endpoint
 * @returns A function that takes an audio blob and returns the transcribed text
 *
 * @example
 * ```typescript
 * const transcribe = createServerTranscriber({
 *   url: 'https://llama.krohnos.io',
 *   model: 'whisper-1',
 *   language: 'auto',
 * });
 * const text = await transcribe(audioBlob);
 * ```
 */
export function createServerTranscriber(
  config: ServerTranscriptionConfig
): (audioBlob: Blob) => Promise<string> {
  return async (audioBlob: Blob): Promise<string> => {
    const formData = new FormData();
    formData.append('file', audioBlob, 'audio.webm');
    formData.append('model', config.model);

    if (config.language !== 'auto') {
      formData.append('language', config.language);
    }

    const response = await fetch(`${config.url}/v1/audio/transcriptions`, {
      method: 'POST',
      body: formData,
    });

    if (!response.ok) {
      throw new Error(`Transcription failed: ${response.status} ${response.statusText}`);
    }

    const result = await response.json();
    return result.text;
  };
}
