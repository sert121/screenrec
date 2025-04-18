export interface RecordingOptions {
    fps: number;
    show_cursor: boolean;
    show_highlight: boolean;
    save_frames: boolean;
    capture_keystrokes: boolean;
}

export interface RecordingState {
    is_recording: boolean;
    duration: number;
    output_path?: string;
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