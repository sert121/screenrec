import React, { useState, useEffect, useRef } from 'react';
import { RecordingOptions, RecordingState } from '../types/recording';
import { RecordingFactory } from '../services/recording-factory.service';

const defaultOptions: RecordingOptions = {
  audio: true,
  video: true,
  frameRate: 30,
  quality: 'high',
};

export const RecordingControls: React.FC = () => {
  const [recordingState, setRecordingState] = useState<RecordingState>({
    isRecording: false,
    duration: 0,
  });
  const [options, setOptions] = useState<RecordingOptions>(defaultOptions);
  const [recordingService, setRecordingService] = useState<any>(null);
  const timerRef = useRef<number | null>(null);

  useEffect(() => {
    const initService = async () => {
      try {
        console.log('Initializing recording service...');
        
        // Use a hardcoded platform value for testing
        const platform = 'macos'; // or 'windows' or 'linux' depending on your OS
        console.log('Using hardcoded platform:', platform);
        
        const service = RecordingFactory.createRecordingService(platform);
        console.log('Recording service created:', service);
        setRecordingService(service);
      } catch (error) {
        console.error('Failed to initialize recording service:', error);
        setRecordingState(prev => ({
          ...prev,
          error: `Failed to initialize: ${String(error)}`
        }));
      }
    };
    
    initService();
    
    return () => {
      if (timerRef.current) {
        clearInterval(timerRef.current);
      }
    };
  }, []);

  const handleStartRecording = async () => {
    try {
      if (!recordingService) {
        console.error('Recording service not initialized');
        return;
      }
      
      await recordingService.startRecording(options);
      
      // Update recording state
      setRecordingState({
        isRecording: true,
        duration: 0,
      });
      
      // Start timer
      timerRef.current = window.setInterval(() => {
        setRecordingState(prev => ({
          ...prev,
          duration: prev.duration + 1
        }));
      }, 1000);
      
    } catch (error) {
      console.error('Failed to start recording:', error);
      setRecordingState(prev => ({
        ...prev,
        error: String(error)
      }));
    }
  };

  const handleStopRecording = async () => {
    try {
      if (!recordingService) {
        console.error('Recording service not initialized');
        return;
      }
      
      // Clear timer
      if (timerRef.current) {
        clearInterval(timerRef.current);
        timerRef.current = null;
      }
      
      const filePath = await recordingService.stopRecording();
      
      // Update recording state
      setRecordingState({
        isRecording: false,
        duration: 0,
      });
      
      console.log('Recording saved to:', filePath);
    } catch (error) {
      console.error('Failed to stop recording:', error);
      setRecordingState(prev => ({
        ...prev,
        error: String(error)
      }));
    }
  };

  const formatDuration = (seconds: number): string => {
    const mins = Math.floor(seconds / 60);
    const secs = seconds % 60;
    return `${mins.toString().padStart(2, '0')}:${secs.toString().padStart(2, '0')}`;
  };

  return (
    <div className="recording-controls">
      <div className="options">
        <label>
          <input
            type="checkbox"
            checked={options.audio}
            onChange={(e) => setOptions({ ...options, audio: e.target.checked })}
          />
          Record Audio
        </label>
        <label>
          <input
            type="checkbox"
            checked={options.video}
            onChange={(e) => setOptions({ ...options, video: e.target.checked })}
          />
          Record Video
        </label>
        <select
          value={options.quality}
          onChange={(e) => setOptions({ ...options, quality: e.target.value as any })}
        >
          <option value="high">High Quality</option>
          <option value="medium">Medium Quality</option>
          <option value="low">Low Quality</option>
        </select>
      </div>

      <div className="controls">
        {!recordingState.isRecording ? (
          <button onClick={handleStartRecording} className="start-btn">
            Start Recording
          </button>
        ) : (
          <button onClick={handleStopRecording} className="stop-btn">
            Stop Recording
          </button>
        )}
      </div>

      {recordingState.isRecording && (
        <div className="recording-info">
          <span className="duration">{formatDuration(recordingState.duration)}</span>
          <span className="recording-indicator">● Recording</span>
        </div>
      )}

      {recordingState.error && (
        <div className="error-message">
          Error: {recordingState.error}
        </div>
      )}
    </div>
  );
}; 