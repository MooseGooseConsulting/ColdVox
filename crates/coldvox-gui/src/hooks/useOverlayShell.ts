import { useCallback, useEffect, useState, useRef } from "react";
import { listen } from "@tauri-apps/api/event";
import {
  DEFAULT_SNAPSHOT,
  type OverlaySnapshot,
} from "../contracts/overlay";
import {
  clearOverlayTranscript,
  getOverlaySnapshot,
  openSettingsPlaceholder,
  setOverlayExpanded,
  startPipeline,
  stopPipeline,
  subscribeToOverlayEvents,
  togglePauseState,
  updatePartialTranscript,
  updateFinalTranscript,
  setOverlayProcessing,
  setOverlayListening,
  stopOverlayCapture,
} from "../lib/overlayBridge";

function messageFromError(error: unknown): string {
  if (error instanceof Error) {
    return error.message;
  }

  if (typeof error === "string") {
    return error;
  }

  return "Unknown bridge failure.";
}

export function useOverlayShell() {
  const [snapshot, setSnapshot] = useState<OverlaySnapshot>(DEFAULT_SNAPSHOT);
  // Live mic input level (0..1 RMS), forwarded from the Tauri backend's
  // audio-frame subscription. rAF-throttled on the render side so per-frame
  // emits from Rust don't flood React state.
  const [micLevel, setMicLevel] = useState(0);

  useEffect(() => {
    let active = true;

    void getOverlaySnapshot()
      .then((initialSnapshot) => {
        if (active) {
          setSnapshot(initialSnapshot);
        }
      })
      .catch((error: unknown) => {
        if (active) {
          setSnapshot((current) => ({
            ...current,
            expanded: true,
            status: "error",
            statusDetail: "Unable to reach the Tauri host shell.",
            errorMessage: messageFromError(error),
          }));
        }
      });

    const unlistenPromise = subscribeToOverlayEvents((event) => {
      if (active) {
        setSnapshot(event.snapshot);
      }
    });

    return () => {
      active = false;
      void unlistenPromise.then((unlisten) => {
        unlisten();
      });
    };
  }, []);

  // Mic-level meter: subscribe to the backend's `mic-level` event (a normalized
  // RMS per audio frame) and rAF-throttle state writes so high-frequency emits
  // don't trigger a React re-render per frame. In dev (vitest / vite without
  // the Tauri host) `listen` rejects because there is no IPC bridge — the
  // `.catch()` swallows that so the hook degrades to a silent no-op meter
  // instead of surfacing an unhandled promise rejection on every mount.
  useEffect(() => {
    let active = true;
    let pending: number | null = null;
    let raf = 0;

    const flush = () => {
      raf = 0;
      if (pending !== null && active) {
        setMicLevel(pending);
        pending = null;
      }
    };

    const unlistenPromise = listen<number>("mic-level", (event) => {
      pending = event.payload;
      if (raf === 0) {
        raf = requestAnimationFrame(flush);
      }
    }).catch(() => {
      return () => {};
    });

    return () => {
      active = false;
      if (raf !== 0) {
        cancelAnimationFrame(raf);
      }
      void unlistenPromise.then((unlisten) => {
        unlisten();
      });
    };
  }, []);

  // Decay the meter toward zero when not actively listening so a frozen last
  // value doesn't linger on screen.
  useEffect(() => {
    if (snapshot.status === "listening" || snapshot.status === "processing") {
      return;
    }
    setMicLevel(0);
  }, [snapshot.status]);

  const runCommand = useCallback(
    async (command: () => Promise<OverlaySnapshot>) => {
      try {
        const nextSnapshot = await command();
        setSnapshot(nextSnapshot);
      } catch (error: unknown) {
        setSnapshot((current) => ({
          ...current,
          expanded: true,
          status: "error",
          statusDetail: "The host shell rejected the latest command.",
          errorMessage: messageFromError(error),
        }));
      }
    },
    [],
  );

  // Debounce-flush partial transcript updates to avoid flooding the shell on rapid STT output.
  const pendingPartialRef = useRef<string | null>(null);
  const flushTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const flushPartial = useCallback(() => {
    const text = pendingPartialRef.current;
    if (text === null) return;
    pendingPartialRef.current = null;
    if (flushTimerRef.current !== null) {
      clearTimeout(flushTimerRef.current);
      flushTimerRef.current = null;
    }
    void runCommand(() => updatePartialTranscript(text));
  }, [runCommand]);

  const queuePartialTranscript = useCallback(
    (text: string) => {
      pendingPartialRef.current = text;
      if (flushTimerRef.current !== null) {
        clearTimeout(flushTimerRef.current);
      }
      // Flush after 80 ms of no new partials — balances latency vs. reduce repaints.
      flushTimerRef.current = setTimeout(flushPartial, 80);
    },
    [flushPartial],
  );

  const cancelPendingPartial = useCallback(() => {
    pendingPartialRef.current = null;
    if (flushTimerRef.current !== null) {
      clearTimeout(flushTimerRef.current);
      flushTimerRef.current = null;
    }
  }, []);

  // Cancel any pending partial flush when the component unmounts.
  useEffect(() => {
    return () => {
      cancelPendingPartial();
    };
  }, [cancelPendingPartial]);

  return {
    snapshot,
    micLevel,
    setExpanded: (expanded: boolean) => runCommand(() => setOverlayExpanded(expanded)),
    startPipeline: () => runCommand(startPipeline),
    togglePause: () => runCommand(togglePauseState),
    stopPipeline: () => runCommand(stopPipeline),
    clearTranscript: () => runCommand(clearOverlayTranscript),
    openSettings: () => runCommand(openSettingsPlaceholder),
    // Pipeline wiring — for real STT integration.
    // queuePartialTranscript debounces rapid partials; flushPartial sends immediately.
    queuePartialTranscript,
    updateFinalTranscript: (text: string) => {
      cancelPendingPartial();
      return runCommand(() => updateFinalTranscript(text));
    },
    setOverlayProcessing: () => {
      cancelPendingPartial();
      return runCommand(setOverlayProcessing);
    },
    setOverlayListening: () => {
      cancelPendingPartial();
      return runCommand(setOverlayListening);
    },
    stopOverlayCapture: () => {
      cancelPendingPartial();
      return runCommand(stopOverlayCapture);
    },
  };
}
