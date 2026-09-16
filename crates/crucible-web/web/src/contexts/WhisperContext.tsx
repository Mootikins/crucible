import {
  createContext,
  useContext,
  ParentComponent,
  createSignal,
  createEffect,
} from 'solid-js';
import { useSettings } from './SettingsContext';
import { createServerTranscriber } from '@/lib/transcription';
import { notificationActions } from '@/stores/notificationStore';

/**
 * Failures this provider already told the user about.
 *
 * The provider rethrows after it reports, so the button that awaited the call
 * sees the same failure. It checks here before reporting again, or a dead
 * transcription server would raise two toasts for one press.
 */
const reported = new WeakSet<object>();

/** Whether `err` was already put in the notification area by the provider. */
export function reportedToUser(err: unknown): boolean {
  return typeof err === 'object' && err !== null && reported.has(err);
}

/** Put a failure in the notification area, once, and hand it back to throw. */
function report(prefix: string, err: unknown): Error {
  const failure = err instanceof Error ? err : new Error(String(err));
  if (!reported.has(failure)) {
    reported.add(failure);
    notificationActions.addNotification('error', `${prefix}: ${failure.message}`);
  }
  return failure;
}

type WhisperStatus = 'idle' | 'loading' | 'ready' | 'error' | 'transcribing';

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

// Pure DSP helpers, extracted from decodeAudioBlob so they're unit-testable
// (Whisper expects 16 kHz mono PCM). Nearest-neighbour resample matches the
// original inline implementation.
export function mixToMono(channels: Float32Array[]): Float32Array {
  if (channels.length <= 1) return channels[0] ?? new Float32Array(0);
  const left = channels[0];
  const right = channels[1];
  const out = new Float32Array(left.length);
  for (let i = 0; i < left.length; i++) {
    out[i] = (left[i] + right[i]) / 2;
  }
  return out;
}

export function resampleTo(
  data: Float32Array,
  fromRate: number,
  toRate: number,
): Float32Array {
  if (fromRate === toRate) return data;
  const ratio = fromRate / toRate;
  const newLength = Math.round(data.length / ratio);
  const out = new Float32Array(newLength);
  for (let i = 0; i < newLength; i++) {
    out[i] = data[Math.floor(i * ratio)];
  }
  return out;
}

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
      const failure = report('Failed to load the speech model', err);
      setError(failure.message);
      setLocalStatus('error');
      throw failure;
    }
  };

  // Convert audio blob to Float32Array at 16kHz (Whisper's expected format)
  const decodeAudioBlob = async (blob: Blob): Promise<Float32Array> => {
    const arrayBuffer = await blob.arrayBuffer();
    const audioContext = new AudioContext({ sampleRate: 16000 });

    try {
      const audioBuffer = await audioContext.decodeAudioData(arrayBuffer);

      const channels =
        audioBuffer.numberOfChannels === 1
          ? [audioBuffer.getChannelData(0)]
          : [audioBuffer.getChannelData(0), audioBuffer.getChannelData(1)];
      const audioData = mixToMono(channels);
      return resampleTo(audioData, audioBuffer.sampleRate, 16000);
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
      // A local model that failed to load was reported by `loadModel`; the
      // guard in `report` keeps it to one notification.
      const failure = report('Transcription failed', err);
      setError(failure.message);
      throw failure;
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

const fallbackWhisperContext: WhisperContextValue = {
  status: () => 'idle',
  progress: () => 0,
  error: () => null,
  transcribe: () => Promise.reject(new Error('Voice input requires a WhisperProvider')),
  loadModel: () => Promise.reject(new Error('Voice input requires a WhisperProvider')),
};

export function useWhisperSafe(): WhisperContextValue {
  const context = useContext(WhisperContext);
  return context ?? fallbackWhisperContext;
}
