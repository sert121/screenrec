import { RecordingControls } from './components/RecordingControls';
import { TauriTest } from './components/TauriTest';
import './styles/RecordingControls.css';

function App() {
  return (
    <div className="container">
      <h1>Screen Recorder</h1>
      <TauriTest />
      <RecordingControls />
    </div>
  );
}

export default App; 