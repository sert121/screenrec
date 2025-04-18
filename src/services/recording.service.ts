import { RecordingOptions, RecordingState, RecordingService } from '../types/recording';
import { invoke } from '@tauri-apps/api/tauri';

export abstract class BaseRecordingService implements RecordingService {
  protected state: RecordingState = {
    isRecording: false,
    duration: 0,
  };

  constructor() {
    this.startDurationTimer = this.startDurationTimer.bind(this);
    this.stopDurationTimer = this.stopDurationTimer.bind(this);
  }

  protected durationTimer: NodeJS.Timer | null = null;

  protected startDurationTimer(): void {
    if (!this.durationTimer) {
      this.durationTimer = setInterval(() => {
        this.state.duration += 1;
      }, 1000);
    }
  }

  protected stopDurationTimer(): void {
    if (this.durationTimer) {
      clearInterval(this.durationTimer);
      this.durationTimer = null;
    }
  }

  abstract startRecording(options: RecordingOptions): Promise<void>;
  abstract stopRecording(): Promise<string>;
  abstract pauseRecording(): Promise<void>;
  abstract resumeRecording(): Promise<void>;

  getState(): RecordingState {
    return { ...this.state };
  }

  protected async invokeCommand(command: string, args?: any): Promise<any> {
    try {
      return await invoke(command, args);
    } catch (error) {
      this.state.error = error instanceof Error ? error.message : 'Unknown error occurred';
      throw error;
    }
  }
} 