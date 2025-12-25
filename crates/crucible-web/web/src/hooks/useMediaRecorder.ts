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

  // Analyze audio levels in real-time using peak detection
  const updateAudioLevel = () => {
    if (!analyser) return;

    const dataArray = new Uint8Array(analyser.fftSize);
    analyser.getByteTimeDomainData(dataArray);

    // Peak detection - find max deviation from center (128)
    let peak = 0;
    for (let i = 0; i < dataArray.length; i++) {
      const amplitude = Math.abs(dataArray[i] - 128);
      if (amplitude > peak) {
        peak = amplitude;
      }
    }

    // Normalize peak to 0-1 with high gain and exponential curve for dramatic response
    // Peak range is 0-128, normalize first
    const normalized = Math.min(1, (peak / 128) * 3); // 3x gain on raw signal
    // Exponential curve (x^2): quiet stays quiet, loud EXPLODES
    // This is the opposite of sqrt - makes variations more dramatic
    const level = normalized * normalized; // x² for punchy response
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
      analyser.fftSize = 256; // Good balance of speed and accuracy
      analyser.smoothingTimeConstant = 0; // NO smoothing - raw values only

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
