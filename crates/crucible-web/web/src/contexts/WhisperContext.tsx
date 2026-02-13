import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  createEffect,
} from 'solid-js';
import { useSettings } from './SettingsContext';
import { createServerTranscriber } from '@/lib/transcription';

export type WhisperStatus = 'idle' | 'loading' | 'ready' | 'error' | 'transcribing';

export interface WhisperContextValue {
  status: () => WhisperStatus;
  progress: () => number;
  error: () => string | null;
  transcribe: (audioBlob: Blob) => Promise<string>;
  loadModel: () => Promise<void>;
}

const WhisperContext = createContext<WhisperContextValue>();

let pipelineFactory: ((...args: unknown[]) => Promise<unknown>) | null = null;
let transcriber: ((...args: unknown[]) => Promise<unknown>) | null = null;

export const WhisperProvider: ParentComponent = (props) => {
  const { settings } = useSettings();
  const [localStatus, setLocalStatus] = createSignal<WhisperStatus>('idle');
  const [progress, setProgress] = createSignal(0);
  const [error, setError] = createSignal<string | null>(null);

  // Compute effective status based on provider
  // Server provider is always "ready" since no model loading is needed
  const status = (): WhisperStatus => {
    if (settings.transcription.provider === 'server') {
      // For server, only show transcribing state, otherwise ready
      return localStatus() === 'transcribing' ? 'transcribing' : 'ready';
    }
    return localStatus();
  };

  // Reset status when provider changes
  createEffect(() => {
    void settings.transcription.provider;
    setError(null);
  });

  const loadModel = async (): Promise<void> => {
    // Skip loading for server provider
    if (settings.transcription.provider === 'server') {
      return;
    }

    if (localStatus() === 'ready' || localStatus() === 'loading') {
      return;
    }

    setLocalStatus('loading');
    setError(null);
    setProgress(0);

    try {
      // Dynamic import of transformers.js
      if (!pipelineFactory) {
        const transformers = await import('@huggingface/transformers');
        pipelineFactory = transformers.pipeline as (...args: unknown[]) => Promise<unknown>;
      }

      // Detect WebGPU support
      const hasWebGPU = typeof navigator !== 'undefined' && 'gpu' in navigator;
      const device = hasWebGPU ? 'webgpu' : 'wasm';
      console.log(`Loading Whisper with device: ${device}`);

      // Load the Whisper model
      // Using whisper-tiny for faster loading, can upgrade to whisper-base for better quality
      transcriber = (await pipelineFactory(
        'automatic-speech-recognition',
        'onnx-community/whisper-tiny.en',
        {
          device,
          progress_callback: (progressData: { progress?: number; status?: string }) => {
            if (progressData.progress !== undefined) {
              setProgress(Math.round(progressData.progress));
            }
          },
        }
      )) as (...args: unknown[]) => Promise<unknown>;

      setProgress(100);
      setLocalStatus('ready');
    } catch (err) {
      console.error('Failed to load Whisper model:', err);
      setError(err instanceof Error ? err.message : 'Failed to load speech model');
      setLocalStatus('error');
      throw err;
    }
  };

  // Convert audio blob to Float32Array at 16kHz (Whisper's expected format)
  const decodeAudioBlob = async (blob: Blob): Promise<Float32Array> => {
    const arrayBuffer = await blob.arrayBuffer();
    const audioContext = new AudioContext({ sampleRate: 16000 });

    try {
      const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);

      // Get mono channel (use first channel or mix down)
      let audioData: Float32Array;
      if (audioBuffer.numberOfChannels === 1) {
        audioData = audioBuffer.getChannelData(0);
      } else {
        // Mix stereo to mono
        const left = audioBuffer.getChannelData(0);
        const right = audioBuffer.getChannelData(1);
        audioData = new Float32Array(left.length);
        for (let i = 0; i < left.length; i++) {
          audioData[i] = (left[i] + right[i]) / 2;
        }
      }

      // Resample to 16kHz if needed
      if (audioBuffer.sampleRate !== 16000) {
        const ratio = audioBuffer.sampleRate / 16000;
        const newLength = Math.round(audioData.length / ratio);
        const resampled = new Float32Array(newLength);
        for (let i = 0; i < newLength; i++) {
          const srcIndex = Math.floor(i * ratio);
          resampled[i] = audioData[srcIndex];
        }
        return resampled;
      }

      return audioData;
    } finally {
      await audioContext.close();
    }
  };

  // Transcribe using server-side OpenAI-compatible endpoint
  const transcribeServer = async (audioBlob: Blob): Promise<string> => {
    const transcribeFunc = createServerTranscriber({
      url: settings.transcription.serverUrl,
      model: settings.transcription.model,
      language: settings.transcription.language,
    });
    return transcribeFunc(audioBlob);
  };

  // Transcribe using local transformers.js model
  const transcribeLocal = async (audioBlob: Blob): Promise<string> => {
    if (localStatus() !== 'ready' || !transcriber) {
      // Auto-load model if not ready
      await loadModel();
    }

    if (!transcriber) {
      throw new Error('Whisper model not loaded');
    }

    // Decode audio blob to Float32Array at 16kHz
    const audioData = await decodeAudioBlob(audioBlob);
    console.log(`Audio decoded: ${audioData.length} samples at 16kHz (${(audioData.length / 16000).toFixed(2)}s)`);

    // Transcribe the audio (no language/task options for English-only model)
    const result = await transcriber(audioData);

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
  };

  const transcribe = async (audioBlob: Blob): Promise<string> => {
    setLocalStatus('transcribing');

    try {
      let result: string;

      if (settings.transcription.provider === 'server') {
        result = await transcribeServer(audioBlob);
      } else {
        result = await transcribeLocal(audioBlob);
      }

      setLocalStatus('ready');
      return result;
    } catch (err) {
      console.error('Transcription failed:', err);
      setLocalStatus(settings.transcription.provider === 'server' ? 'ready' : 'error');
      setError(err instanceof Error ? err.message : 'Transcription failed');
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

const fallbackWhisperContext: WhisperContextValue = {
  status: () => 'idle',
  progress: () => 0,
  error: () => null,
  transcribe: () => Promise.resolve(''),
  loadModel: () => Promise.resolve(),
};

export function useWhisperSafe(): WhisperContextValue {
  const context = useContext(WhisperContext);
  return context ?? fallbackWhisperContext;
}
