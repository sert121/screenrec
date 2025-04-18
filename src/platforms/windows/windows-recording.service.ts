import { BaseRecordingService } from '../../services/recording.service';
import { RecordingOptions } from '../../types/recording';
import { appDataDir, join } from '@tauri-apps/api/path';
import { Command } from '@tauri-apps/api/shell';

export class WindowsRecordingService extends BaseRecordingService {
  private command: Command | null = null;
  private outputPath: string = '';

  async startRecording(options: RecordingOptions): Promise<void> {
    try {
      const timestamp = new Date().toISOString().replace(/[:.]/g, '-');
      const dataDir = await appDataDir();
      this.outputPath = await join(dataDir, `recording-${timestamp}.mp4`);

      const args = this.buildXboxGameBarArgs(options);
      this.command = new Command('powershell', args);
      
      await this.command.execute();
      this.state.isRecording = true;
      this.startDurationTimer();
    } catch (error) {
      this.state.error = error instanceof Error ? error.message : 'Failed to start recording';
      throw error;
    }
  }

  async stopRecording(): Promise<string> {
    if (!this.command) {
      throw new Error('No recording in progress');
    }

    try {
      await this.command.kill();
      this.state.isRecording = false;
      this.stopDurationTimer();
      return this.outputPath;
    } catch (error) {
      this.state.error = error instanceof Error ? error.message : 'Failed to stop recording';
      throw error;
    }
  }

  async pauseRecording(): Promise<void> {
    if (!this.command) {
      throw new Error('No recording in progress');
    }

    try {
      await this.invokeCommand('windows-pause-recording');
    } catch (error) {
      this.state.error = error instanceof Error ? error.message : 'Failed to pause recording';
      throw error;
    }
  }

  async resumeRecording(): Promise<void> {
    if (!this.command) {
      throw new Error('No recording in progress');
    }

    try {
      await this.invokeCommand('windows-resume-recording');
    } catch (error) {
      this.state.error = error instanceof Error ? error.message : 'Failed to resume recording';
      throw error;
    }
  }

  private buildXboxGameBarArgs(options: RecordingOptions): string[] {
    const args: string[] = [
      '-Command',
      'Start-Process',
      '"shell:AppsFolder\Microsoft.XboxGameBar_8wekyb3d8bbwe!App"',
      '-ArgumentList',
      '"--record"'
    ];

    if (options.region) {
      args.push(
        '--region',
        `${options.region.x},${options.region.y},${options.region.width},${options.region.height}`
      );
    }

    if (options.audio) {
      args.push('--audio');
    }

    // Add quality settings
    switch (options.quality) {
      case 'high':
        args.push('--quality', 'high');
        break;
      case 'medium':
        args.push('--quality', 'medium');
        break;
      case 'low':
        args.push('--quality', 'low');
        break;
    }

    args.push('--output', this.outputPath);
    return args;
  }
} 