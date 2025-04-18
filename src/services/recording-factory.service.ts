import { invoke } from '@tauri-apps/api/core';
import { RecordingOptions, RecordingState } from '../types/recording';

export class RecordingFactory {
  static createRecordingService(platform: string) {
    console.log('Creating recording service for platform:', platform);
    
    if (!platform) {
      throw new Error('Platform is required to create recording service');
    }
    
    const service = {
      startRecording: async (options: RecordingOptions) => {
        console.log('Starting recording with options:', options);
        await invoke('start_recording', { options });
        console.log('Recording started successfully');
      },
      
      stopRecording: async () => {
        console.log('Stopping recording');
        const outputPath = await invoke<string>('stop_recording');
        console.log('Recording stopped, saved to:', outputPath);
        return outputPath;
      },
      
      getState: async (): Promise<RecordingState> => {
        try {
          console.log('Getting recording state from backend');
          const state = await invoke<RecordingState>('get_recording_state');
          console.log('Recording state:', state);
          return state;
        } catch (error) {
          console.error('Failed to get recording state:', error);
          return {
            isRecording: false,
            duration: 0,
            outputPath: undefined,
            error: String(error)
          };
        }
      }
    };
    
    console.log('Recording service created successfully');
    return service;
  }
} 