export interface RecordingOptions {
    audio: boolean;
    video: boolean;
    frameRate: number;
    quality: 'high' | 'medium' | 'low';
    region?: {
      x: number;
      y: number;
      width: number;
      height: number;
    };
  }
  
  export interface RecordingState {
    isRecording: boolean;
    duration: number;
    filePath?: string;
    error?: string;
  }
  
  export interface RecordingService {
    startRecording(options: RecordingOptions): Promise<void>;
    stopRecording(): Promise<string>;
    pauseRecording(): Promise<void>;
    resumeRecording(): Promise<void>;
    getState(): RecordingState;
  }
  
  export type Platform = 'windows' | 'mac';
  
  export interface RecordingStats {
    fps: number;
    bitrate: number;
    fileSize: number;
    duration: number;
  } 