export interface RecordingOptions {
    fps: number;
    output_path: string;
    audio: boolean;
    video: boolean;
    frame_rate: number;
    quality: string;
}

export interface RecordingState {
    isRecording: boolean;
    duration: number;
    outputPath?: string;
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