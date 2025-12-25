import { createSignal } from 'solid-js';

export interface UseMediaRecorderResult {
  isRecording: () => boolean;
  error: () => string | null;
  startRecording: () => Promise<void>;
  stopRecording: () => Promise<Blob>;
}

/**
 * Hook for capturing audio using the MediaRecorder API.
 * Returns controls for starting/stopping recording and accessing the audio blob.
 */
export function useMediaRecorder(): UseMediaRecorderResult {
  const [isRecording, setIsRecording] = createSignal(false);
  const [error, setError] = createSignal<string | null>(null);

  let mediaRecorder: MediaRecorder | null = null;
  let audioChunks: Blob[] = [];
  let resolveStop: ((blob: Blob) => void) | null = null;

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

  return {
    isRecording,
    error,
    startRecording,
    stopRecording,
  };
}
