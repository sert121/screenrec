import React, { useState, useEffect } from 'react';
import { invoke } from '@tauri-apps/api/core';

export const TauriTest: React.FC = () => {
  const [greetResult, setGreetResult] = useState<string>('');
  const [platformResult, setPlatformResult] = useState<string>('');
  const [error, setError] = useState<string>('');

  const testGreet = async () => {
    try {
      const result = await invoke<string>('greet', { name: 'test' });
      setGreetResult(result);
      setError('');
    } catch (err) {
      setError(`Greet error: ${String(err)}`);
    }
  };

  const testGetPlatform = async () => {
    try {
      const result = await invoke<string>('get_platform');
      setPlatformResult(result);
      setError('');
    } catch (err) {
      setError(`Platform error: ${String(err)}`);
    }
  };

  return (
    <div className="tauri-test">
      <h2>Tauri Command Test</h2>
      <div>
        <button onClick={testGreet}>Test Greet</button>
        {greetResult && <p>Greet result: {greetResult}</p>}
      </div>
      <div>
        <button onClick={testGetPlatform}>Test Get Platform</button>
        {platformResult && <p>Platform result: {platformResult}</p>}
      </div>
      {error && <p className="error">{error}</p>}
    </div>
  );
}; 