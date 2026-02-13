import { describe, it, expect, vi, beforeEach, afterEach } from 'vitest';
import { createRoot } from 'solid-js';
import { useMediaRecorder } from './useMediaRecorder';

// Mock MediaRecorder
class MockMediaRecorder {
  state: 'inactive' | 'recording' = 'inactive';
  ondataavailable: ((event: { data: Blob }) => void) | null = null;
  onstop: (() => void) | null = null;
  onerror: ((event: Event) => void) | null = null;

  constructor(
    private stream: MediaStream,
    _options?: { mimeType: string }
  ) {}

  start(_timeslice?: number) {
    this.state = 'recording';
    // Simulate some audio data
    setTimeout(() => {
      if (this.ondataavailable) {
        this.ondataavailable({ data: new Blob(['audio chunk'], { type: 'audio/webm' }) });
      }
    }, 10);
  }

  stop() {
    this.state = 'inactive';
    // Stop tracks
    this.stream.getTracks().forEach((track) => track.stop());
    if (this.onstop) {
      this.onstop();
    }
  }

  static isTypeSupported(mimeType: string): boolean {
    return mimeType === 'audio/webm;codecs=opus' || mimeType === 'audio/webm';
  }
}

// Mock MediaStream
class MockMediaStream {
  private tracks: { stop: () => void }[] = [{ stop: vi.fn() }];

  getTracks() {
    return this.tracks;
  }
}

// Mock navigator.mediaDevices
const mockGetUserMedia = vi.fn();

// Mock AnalyserNode for audio level analysis
class MockAnalyserNode {
  fftSize = 256;
  smoothingTimeConstant = 0.5;
  getByteTimeDomainData(array: Uint8Array) {
    // Fill with silence (128 = zero level)
    array.fill(128);
  }
}

// Mock AudioContext with analyser support
class MockAudioContext {
  createAnalyser() {
    return new MockAnalyserNode();
  }
  createMediaStreamSource() {
    return { connect: vi.fn() };
  }
  close() {
    return Promise.resolve();
  }
}

describe('useMediaRecorder', () => {
  beforeEach(() => {
    vi.stubGlobal('MediaRecorder', MockMediaRecorder);
    vi.stubGlobal('AudioContext', MockAudioContext);
    vi.stubGlobal('navigator', {
      mediaDevices: {
        getUserMedia: mockGetUserMedia,
      },
    });
    mockGetUserMedia.mockResolvedValue(new MockMediaStream());
  });

  afterEach(() => {
    vi.restoreAllMocks();
  });

  it('starts in non-recording state with zero audio level', () => {
    createRoot((dispose) => {
      const { isRecording, error, audioLevel } = useMediaRecorder();
      expect(isRecording()).toBe(false);
      expect(error()).toBeNull();
      expect(audioLevel()).toBe(0);
      dispose();
    });
  });

  it('requests microphone access on startRecording', async () => {
    await createRoot(async (dispose) => {
      const { startRecording } = useMediaRecorder();
      await startRecording();

      expect(mockGetUserMedia).toHaveBeenCalledWith({
        audio: {
          channelCount: 1,
          sampleRate: 16000,
        },
      });
      dispose();
    });
  });

  it('sets isRecording to true after starting', async () => {
    await createRoot(async (dispose) => {
      const { startRecording, isRecording } = useMediaRecorder();
      await startRecording();

      expect(isRecording()).toBe(true);
      dispose();
    });
  });

  it('returns audio blob on stopRecording', async () => {
    await createRoot(async (dispose) => {
      const { startRecording, stopRecording } = useMediaRecorder();
      await startRecording();

      // Wait for data to be collected
      await new Promise((r) => setTimeout(r, 20));

      const blob = await stopRecording();
      expect(blob).toBeInstanceOf(Blob);
      dispose();
    });
  });

  it('sets isRecording to false after stopping', async () => {
    await createRoot(async (dispose) => {
      const { startRecording, stopRecording, isRecording } = useMediaRecorder();
      await startRecording();
      await stopRecording();

      expect(isRecording()).toBe(false);
      dispose();
    });
  });

  it('handles permission denied error', async () => {
    mockGetUserMedia.mockRejectedValue(new DOMException('Permission denied', 'NotAllowedError'));

    await createRoot(async (dispose) => {
      const { startRecording, error } = useMediaRecorder();

      await expect(startRecording()).rejects.toThrow();
      expect(error()).toBe('Microphone permission denied');
      dispose();
    });
  });

  it('handles no microphone found error', async () => {
    mockGetUserMedia.mockRejectedValue(new DOMException('No device', 'NotFoundError'));

    await createRoot(async (dispose) => {
      const { startRecording, error } = useMediaRecorder();

      await expect(startRecording()).rejects.toThrow();
      expect(error()).toBe('No microphone found');
      dispose();
    });
  });

  it('rejects stopRecording if not recording', async () => {
    await createRoot(async (dispose) => {
      const { stopRecording } = useMediaRecorder();

      await expect(stopRecording()).rejects.toThrow('Not recording');
      dispose();
    });
  });
});
