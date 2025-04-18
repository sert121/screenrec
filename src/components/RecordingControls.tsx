import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';
import { RecordingFactory } from '../services/recording-factory.service';
import { RecordingOptions, RecordingState } from '../types/recording';

export const RecordingControls: React.FC = () => {
  const [isRecording, setIsRecording] = useState(false);
  const [duration, setDuration] = useState(0);
  const [error, setError] = useState<string | null>(null);
  const [recordingPath, setRecordingPath] = useState<string | null>(null);
  const [recordingService, setRecordingService] = useState<any>(null);

  useEffect(() => {
    const initRecordingService = async () => {
      try {
        const platform = await invoke('get_platform');
        const service = RecordingFactory.createRecordingService(platform as string);
        setRecordingService(service);
        
        // Get initial state
        const state = await service.getState();
        setIsRecording(state.is_recording);
        setDuration(state.duration);
        if (state.output_path) {
          setRecordingPath(state.output_path);
        }
      } catch (err) {
        console.error('Failed to initialize recording service:', err);
        setError('Failed to initialize recording service');
      }
    };

    initRecordingService();
    
    // Add visibility change handler to check recording state when tab becomes visible again
    const handleVisibilityChange = async () => {
      if (document.visibilityState === 'visible' && recordingService) {
        try {
          const state = await recordingService.getState();
          setIsRecording(state.is_recording);
          setDuration(state.duration);
          if (state.output_path) {
            setRecordingPath(state.output_path);
          }
        } catch (err) {
          console.error('Failed to get recording state after tab switch:', err);
        }
      }
    };
    
    document.addEventListener('visibilitychange', handleVisibilityChange);
    
    return () => {
      document.removeEventListener('visibilitychange', handleVisibilityChange);
    };
  }, []);

  useEffect(() => {
    let timer: NodeJS.Timeout;
    if (isRecording) {
      timer = setInterval(() => {
        setDuration(prev => prev + 1);
      }, 1000);
    }
    return () => {
      if (timer) {
        clearInterval(timer);
      }
    };
  }, [isRecording]);

  const handleStartRecording = async () => {
    try {
      setError(null);
      const options: RecordingOptions = {
        fps: 30,
        show_cursor: true,
        show_highlight: true,
        save_frames: true,
        capture_keystrokes: true
      };
      
      await recordingService.startRecording(options);
      setIsRecording(true);
      setDuration(0);
      setRecordingPath(null);
    } catch (err) {
      console.error('Failed to start recording:', err);
      setError(err instanceof Error ? err.message : 'Failed to start recording');
    }
  };

  const handleStopRecording = async () => {
    try {
      setError(null);
      if (!recordingService) {
        throw new Error('Recording service not initialized');
      }
      const path = await recordingService.stopRecording();
      setIsRecording(false);
      setRecordingPath(path);
      
      // Clear recording state after saving
      setDuration(0);
      
      // Reset recording service to get a fresh state
      const platform = await invoke('get_platform');
      const newService = RecordingFactory.createRecordingService(platform as string);
      setRecordingService(newService);
    } catch (err) {
      console.error('Failed to stop recording:', err);
      setError(err instanceof Error ? err.message : 'Failed to stop recording');
      setIsRecording(false); // Ensure recording state is reset even on error
    }
  };

  const formatDuration = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  };

  if (!recordingService) {
    return <div>Loading recording service...</div>;
  }

  return (
    <div className="recording-controls">
      {error && <div className="error">{error}</div>}
      <div className="duration">{formatDuration(duration)}</div>
      <button 
        onClick={isRecording ? handleStopRecording : handleStartRecording}
        className={isRecording ? 'stop' : 'start'}
      >
        {isRecording ? 'Stop Recording' : 'Start Recording'}
      </button>
      {recordingPath && (
        <div className="recording-path">
          Recording saved to: {recordingPath}
        </div>
      )}
    </div>
  );
}; 