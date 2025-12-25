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

      // Detect WebGPU support
      const hasWebGPU = typeof navigator !== 'undefined' && 'gpu' in navigator;
      const device = hasWebGPU ? 'webgpu' : 'wasm';
      console.log(`Loading Whisper with device: ${device}`);

      // Load the Whisper model
      // Using whisper-tiny for faster loading, can upgrade to whisper-base for better quality
      transcriber = await pipeline(
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
      // Decode audio blob to Float32Array at 16kHz
      const audioData = await decodeAudioBlob(audioBlob);
      console.log(`Audio decoded: ${audioData.length} samples at 16kHz (${(audioData.length / 16000).toFixed(2)}s)`);

      // Transcribe the audio (no language/task options for English-only model)
      const result = await transcriber(audioData);

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
