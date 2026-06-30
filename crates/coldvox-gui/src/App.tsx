import { OverlayShell } from "./components/OverlayShell";
import { useOverlayShell } from "./hooks/useOverlayShell";

function App() {
  const {
    snapshot,
    micLevel,
    clearTranscript,
    openSettings,
    setExpanded,
    startPipeline,
    stopPipeline,
    togglePause,
  } = useOverlayShell();

  return (
    <main className="app-frame">
      <OverlayShell
        snapshot={snapshot}
        micLevel={micLevel}
        onSetExpanded={setExpanded}
        onStartDemo={startPipeline}
        onTogglePause={togglePause}
        onStop={stopPipeline}
        onClear={clearTranscript}
        onSettings={openSettings}
      />
    </main>
  );
}

export default App;
