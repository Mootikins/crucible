import { createSignal, onCleanup } from 'solid-js';

export interface UseMediaRecorderResult {
  isRecording: () => boolean;
  error: () => string | null;
  audioLevel: () => number; // 0-1 normalized audio level
  startRecording: () => Promise<void>;
  stopRecording: () => Promise<Blob>;
}

/**
 * Hook for capturing audio using the MediaRecorder API.
 * Returns controls for starting/stopping recording, accessing the audio blob,
 * and real-time audio level for visualization.
 */
export function useMediaRecorder(): UseMediaRecorderResult {
  const [isRecording, setIsRecording] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);
  const [audioLevel, setAudioLevel] = createSignal(0);

  let mediaRecorder: MediaRecorder | null = null;
  let audioContext: AudioContext | null = null;
  let analyser: AnalyserNode | null = null;
  let animationFrameId: number | null = null;
  let audioChunks: Blob[] = [];
  let resolveStop: ((blob: Blob) => void) | null = null;

  // Analyze audio levels in real-time
  const updateAudioLevel = () => {
    if (!analyser) return;

    const dataArray = new Uint8Array(analyser.fftSize);
    analyser.getByteTimeDomainData(dataArray);

    // Calculate RMS (root mean square) for a smoother level
    let sum = 0;
    for (let i = 0; i < dataArray.length; i++) {
      const value = (dataArray[i] - 128) / 128; // Normalize to -1 to 1
      sum += value * value;
    }
    const rms = Math.sqrt(sum / dataArray.length);

    // Normalize to 0-1, with some amplification for better visual response
    const level = Math.min(1, rms * 3);
    setAudioLevel(level);

    if (isRecording()) {
      animationFrameId = requestAnimationFrame(updateAudioLevel);
    }
  };

  const startRecording = async (): Promise<void> => {
    setError(null);
    audioChunks = [];

    try {
      const stream = await navigator.mediaDevices.getUserMedia({
        audio: {
          channelCount: 1,
          sampleRate: 16000, // Whisper expects 16kHz
        },
      });

      // Set up audio analysis
      audioContext = new AudioContext();
      analyser = audioContext.createAnalyser();
      analyser.fftSize = 256;
      analyser.smoothingTimeConstant = 0.5;

      const source = audioContext.createMediaStreamSource(stream);
      source.connect(analyser);
      // Don't connect to destination - we don't want to hear ourselves

      // Prefer webm/opus if available, fallback to whatever is supported
      const mimeType = MediaRecorder.isTypeSupported('audio/webm;codecs=opus')
        ? 'audio/webm;codecs=opus'
        : MediaRecorder.isTypeSupported('audio/webm')
          ? 'audio/webm'
          : 'audio/mp4';

      mediaRecorder = new MediaRecorder(stream, { mimeType });

      mediaRecorder.ondataavailable = (event) => {
        if (event.data.size > 0) {
          audioChunks.push(event.data);
        }
      };

      mediaRecorder.onstop = () => {
        // Stop audio analysis
        if (animationFrameId) {
          cancelAnimationFrame(animationFrameId);
          animationFrameId = null;
        }
        if (audioContext) {
          audioContext.close();
          audioContext = null;
        }
        analyser = null;
        setAudioLevel(0);

        // Stop all tracks to release the microphone
        stream.getTracks().forEach((track) => track.stop());

        const audioBlob = new Blob(audioChunks, { type: mimeType });
        if (resolveStop) {
          resolveStop(audioBlob);
          resolveStop = null;
        }
      };

      mediaRecorder.onerror = (event) => {
        console.error('MediaRecorder error:', event);
        setError('Recording failed');
        setIsRecording(false);
      };

      mediaRecorder.start(100); // Collect data every 100ms
      setIsRecording(true);

      // Start audio level monitoring
      updateAudioLevel();
    } catch (err) {
      console.error('Failed to start recording:', err);
      if (err instanceof DOMException) {
        if (err.name === 'NotAllowedError') {
          setError('Microphone permission denied');
        } else if (err.name === 'NotFoundError') {
          setError('No microphone found');
        } else {
          setError(`Microphone error: ${err.message}`);
        }
      } else {
        setError('Failed to access microphone');
      }
      throw err;
    }
  };

  const stopRecording = (): Promise<Blob> => {
    return new Promise((resolve, reject) => {
      if (!mediaRecorder || mediaRecorder.state === 'inactive') {
        reject(new Error('Not recording'));
        return;
      }

      resolveStop = resolve;
      setIsRecording(false);
      mediaRecorder.stop();
    });
  };

  // Cleanup on unmount
  onCleanup(() => {
    if (animationFrameId) {
      cancelAnimationFrame(animationFrameId);
    }
    if (audioContext) {
      audioContext.close();
    }
  });

  return {
    isRecording,
    error,
    audioLevel,
    startRecording,
    stopRecording,
  };
}
