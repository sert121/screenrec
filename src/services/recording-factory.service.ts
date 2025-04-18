import { invoke } from '@tauri-apps/api/core';
import { RecordingOptions, RecordingState } from '../types/recording';

export class RecordingFactory {
  static createRecordingService(platform: string) {
    console.log('Creating recording service for platform:', platform);
    
    if (!platform) {
      throw new Error('Platform is required to create recording service');
    }
    
    let isRecording = false;
    
    const service = {
      startRecording: async (options: RecordingOptions) => {
        console.log('Starting recording with options:', options);
        await invoke('start_recording', { options });
        isRecording = true;
      },
      stopRecording: async () => {
        console.log('Stopping recording');
        isRecording = false;
        return await invoke('stop_recording');
      },
      getState: (): RecordingState => ({
        isRecording,
        duration: 0
      })
    };
    
    console.log('Recording service created successfully');
    return service;
  }
} 