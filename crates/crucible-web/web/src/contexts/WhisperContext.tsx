import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  onMount,
} from 'solid-js';

export type WhisperStatus = 'idle' | 'loading' | 'ready' | 'error' | 'transcribing';

export interface WhisperContextValue {
  status: () => WhisperStatus;
  progress: () => number;
  error: () => string | null;
  transcribe: (audioBlob: Blob) => Promise<string>;
  loadModel: () => Promise<void>;
}

const WhisperContext = createContext<WhisperContextValue>();

// Dynamic import to avoid loading transformers.js until needed
let pipeline: typeof import('@huggingface/transformers').pipeline | null = null;
let transcriber: Awaited<ReturnType<typeof import('@huggingface/transformers').pipeline>> | null = null;

export const WhisperProvider: ParentComponent = (props) => {
  const [status, setStatus] = createSignal<WhisperStatus>('idle');
  const [progress, setProgress] = createSignal(0);
  const [error, setError] = createSignal<string | null>(null);

  const loadModel = async (): Promise<void> => {
    if (status() === 'ready' || status() === 'loading') {
      return;
    }

    setStatus('loading');
    setError(null);
    setProgress(0);

    try {
      // Dynamic import of transformers.js
      if (!pipeline) {
        const transformers = await import('@huggingface/transformers');
        pipeline = transformers.pipeline;
      }

      // Load the Whisper model
      // Using whisper-tiny for faster loading, can upgrade to whisper-base for better quality
      transcriber = await pipeline(
        'automatic-speech-recognition',
        'onnx-community/whisper-tiny.en',
        {
          device: 'webgpu', // Use WebGPU if available, falls back to WASM
          progress_callback: (progressData: { progress?: number; status?: string }) => {
            if (progressData.progress !== undefined) {
              setProgress(Math.round(progressData.progress));
            }
          },
        }
      );

      setProgress(100);
      setStatus('ready');
    } catch (err) {
      console.error('Failed to load Whisper model:', err);
      setError(err instanceof Error ? err.message : 'Failed to load speech model');
      setStatus('error');
      throw err;
    }
  };

  const transcribe = async (audioBlob: Blob): Promise<string> => {
    if (status() !== 'ready' || !transcriber) {
      // Auto-load model if not ready
      await loadModel();
    }

    if (!transcriber) {
      throw new Error('Whisper model not loaded');
    }

    setStatus('transcribing');

    try {
      // Convert Blob to ArrayBuffer for transcription
      const arrayBuffer = await audioBlob.arrayBuffer();

      // Transcribe the audio
      const result = await transcriber(arrayBuffer, {
        language: 'en',
        task: 'transcribe',
      });

      setStatus('ready');

      // Handle different result formats
      if (typeof result === 'string') {
        return result;
      }
      if (Array.isArray(result)) {
        return result.map((r) => r.text || '').join(' ');
      }
      if (result && typeof result === 'object' && 'text' in result) {
        return (result as { text: string }).text;
      }

      return '';
    } catch (err) {
      console.error('Transcription failed:', err);
      setStatus('ready'); // Reset to ready so user can try again
      throw err;
    }
  };

  const value: WhisperContextValue = {
    status,
    progress,
    error,
    transcribe,
    loadModel,
  };

  return (
    <WhisperContext.Provider value={value}>
      {props.children}
    </WhisperContext.Provider>
  );
};

export function useWhisper(): WhisperContextValue {
  const context = useContext(WhisperContext);
  if (!context) {
    throw new Error('useWhisper must be used within a WhisperProvider');
  }
  return context;
}
