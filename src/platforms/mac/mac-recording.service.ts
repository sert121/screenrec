import { BaseRecordingService } from '../../services/recording.service';
import { RecordingOptions } from '../../types/recording';
import { appDataDir, join } from '@tauri-apps/api/path';
import { Command } from '@tauri-apps/api/shell';

export class MacRecordingService extends BaseRecordingService {
  private command: Command | null = null;
  private outputPath: string = '';

  async startRecording(options: RecordingOptions): Promise<void> {
    try {
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const dataDir = await appDataDir();
      console.log('App data directory:', dataDir);
      
      this.outputPath = await join(dataDir, `recording-${timestamp}.mp4`);
      console.log('Output path:', this.outputPath);

      const args = this.buildScreencaptureArgs(options);
      console.log('Screencapture arguments:', args);
      
      this.command = new Command('screencapture', args);
      console.log('Created screencapture command');
      
      console.log('Executing screencapture command...');
      await this.command.execute();
      console.log('Screencapture command executed successfully');
      
      this.state.isRecording = true;
      this.startDurationTimer();
    } catch (error) {
      console.error('Failed to start recording:', error);
      this.state.error = error instanceof Error ? error.message : 'Failed to start recording';
      throw error;
    }
  }

  async stopRecording(): Promise<string> {
    if (!this.command) {
      console.error('No recording command found');
      throw new Error('No recording in progress');
    }

    try {
      console.log('Attempting to kill screencapture process...');
      await this.command.kill();
      console.log('Screencapture process killed successfully');
      
      this.state.isRecording = false;
      this.stopDurationTimer();
      
      console.log('Returning output path:', this.outputPath);
      return this.outputPath;
    } catch (error) {
      console.error('Failed to stop recording:', error);
      this.state.error = error instanceof Error ? error.message : 'Failed to stop recording';
      throw error;
    }
  }

  async pauseRecording(): Promise<void> {
    // macOS screencapture doesn't support pause/resume
    throw new Error('Pause/resume not supported on macOS');
  }

  async resumeRecording(): Promise<void> {
    // macOS screencapture doesn't support pause/resume
    throw new Error('Pause/resume not supported on macOS');
  }

  private buildScreencaptureArgs(options: RecordingOptions): string[] {
    const args: string[] = ['-v'];

    if (options.region) {
      args.push(
        '-R',
        `${options.region.x},${options.region.y},${options.region.width},${options.region.height}`
      );
    }

    if (options.audio) {
      args.push('-a');
    }

    // Add quality settings
    switch (options.quality) {
      case 'high':
        args.push('-q', '100');
        break;
      case 'medium':
        args.push('-q', '75');
        break;
      case 'low':
        args.push('-q', '50');
        break;
    }

    args.push(this.outputPath);
    return args;
  }
} 