import { useEffect, useMemo, useRef } from "react";
import { type OverlaySnapshot } from "../contracts/overlay";
import { StatusPill } from "./StatusPill";

interface OverlayShellProps {
  snapshot: OverlaySnapshot;
  micLevel?: number;
  onSetExpanded: (expanded: boolean) => Promise<void>;
  onStartDemo: () => Promise<void>;
  onTogglePause: () => Promise<void>;
  onStop: () => Promise<void>;
  onClear: () => Promise<void>;
  onSettings: () => Promise<void>;
}

const STATE_NOTES = {
  idle: "Collapsed idle presence with an explicit expansion path.",
  listening: "Streaming partial text remains visible while speech is active.",
  processing: "Partial words stay nearby while the shell stages a final result.",
  ready: "Final text is visually promoted, but nothing is injected yet.",
  error: "Command rejection or shell issues surface without hiding prior text.",
} as const;

export function OverlayShell({
  snapshot,
  micLevel = 0,
  onSetExpanded,
  onStartDemo,
  onTogglePause,
  onStop,
  onClear,
  onSettings,
}: OverlayShellProps) {
  const transcriptRef = useRef<HTMLDivElement | null>(null);

  useEffect(() => {
    if (transcriptRef.current) {
      transcriptRef.current.scrollTop = transcriptRef.current.scrollHeight;
    }
  }, [snapshot.finalTranscript, snapshot.partialTranscript, snapshot.errorMessage]);

  const collapsedSummary = useMemo(() => {
    if (snapshot.partialTranscript) {
      return snapshot.partialTranscript;
    }

    if (snapshot.finalTranscript) {
      return snapshot.finalTranscript;
    }

    return snapshot.statusDetail;
  }, [snapshot.finalTranscript, snapshot.partialTranscript, snapshot.statusDetail]);

  const pauseLabel = snapshot.paused ? "Resume" : "Pause";
  const showRunDemo =
    snapshot.status === "idle" ||
    snapshot.status === "ready" ||
    snapshot.status === "error";

  // Boost faint signals so quiet speech still moves the meter visibly. Curved
  // (pow 0.6) so low-end sensitivity is high without clipping loud peaks.
  const meterPct = Math.min(100, Math.pow(micLevel, 0.6) * 140);
  const reducedMotion =
    typeof window !== "undefined" &&
    window.matchMedia?.("(prefers-reduced-motion: reduce)").matches;

  if (!snapshot.expanded) {
    return (
      <button
        type="button"
        className="overlay-card overlay-card--collapsed"
        onClick={() => {
          void onSetExpanded(true);
        }}
      >
        <div className="collapsed-shell__mark" aria-hidden="true" />
        <div className="collapsed-shell__copy">
          <span className="eyebrow">Phase 3A Host Shell</span>
          <strong>ColdVox overlay ready</strong>
          <span className="collapsed-shell__detail">{collapsedSummary}</span>
        </div>
        <div className="collapsed-shell__status">
          <StatusPill status={snapshot.status} />
          <span className="collapsed-shell__action">Expand</span>
        </div>
      </button>
    );
  }

  return (
    <section className="overlay-card overlay-card--expanded" aria-label="ColdVox overlay shell">
      <header className="overlay-header" data-tauri-drag-region>
        <div className="header-column header-column--identity">
          <span className="eyebrow">Windows-first transparent overlay</span>
          <h1>ColdVox</h1>
          <p>{snapshot.statusDetail}</p>
        </div>

        <div className="header-column header-column--status">
          <StatusPill status={snapshot.status} />
          <span className="status-caption">{STATE_NOTES[snapshot.status]}</span>
          {snapshot.status === "listening" || snapshot.status === "processing" ? (
            <div
              className="mic-meter"
              role="meter"
              aria-label="Microphone input level"
              aria-valuemin={0}
              aria-valuemax={100}
              aria-valuenow={Math.round(meterPct)}
            >
              <div
                className="mic-meter__fill"
                style={{ width: `${reducedMotion ? Math.round(meterPct) : meterPct}%` }}
              />
            </div>
          ) : null}
        </div>

        <button
          type="button"
          className="ghost-button"
          onClick={() => {
            void onSetExpanded(false);
          }}
        >
          Collapse
        </button>
      </header>

      <div className="overlay-body">
        <aside className="signal-rail" aria-label="Overlay state feedback">
          <div className="signal-rail__section">
            <span className="signal-rail__label">Phase</span>
            <div className="signal-stack" aria-hidden="true">
              <span className={`signal-bar signal-bar--1 signal-bar--${snapshot.status}`} />
              <span className={`signal-bar signal-bar--2 signal-bar--${snapshot.status}`} />
              <span className={`signal-bar signal-bar--3 signal-bar--${snapshot.status}`} />
              <span className={`signal-bar signal-bar--4 signal-bar--${snapshot.status}`} />
            </div>
          </div>

          <div className="signal-rail__section">
            <span className="signal-rail__label">Contract</span>
            <p>
              Commands resize the shell and update state. Events stream transcript
              deltas back into React.
            </p>
          </div>

          <div className="signal-rail__section">
            <span className="signal-rail__label">Scope</span>
            <p>
              Demo only. No STT runtime, hotkeys, injection, or packaging work in
              this tranche.
            </p>
          </div>
        </aside>

        <section className="transcript-panel">
          <div className="transcript-panel__header">
            <div>
              <span className="eyebrow">Transcript</span>
              <h2>Committed words stay dominant</h2>
            </div>
            {snapshot.errorMessage ? (
              <div className="error-badge" role="alert">
                {snapshot.errorMessage}
              </div>
            ) : null}
          </div>

          <div className="transcript-surface" ref={transcriptRef}>
            <section className="transcript-block transcript-block--final">
              <span className="transcript-block__label">Final text</span>
              <p data-testid="final-transcript">
                {snapshot.finalTranscript ||
                  "Final transcript will land here once the demo promotes it out of the partial stream."}
              </p>
            </section>

            <section className="transcript-block transcript-block--partial">
              <span className="transcript-block__label">Live partials</span>
              <p data-testid="partial-transcript">
                {snapshot.partialTranscript ||
                  "Listening state keeps provisional text visible without treating it as committed."}
              </p>
            </section>
          </div>
        </section>
      </div>

      <footer className="overlay-footer">
        <div className="overlay-footer__group overlay-footer__group--primary">
          {showRunDemo ? (
            <button
              type="button"
              className="control-button control-button--primary"
              onClick={() => {
                void onStartDemo();
              }}
            >
              Run demo
            </button>
          ) : null}
          <button
            type="button"
            className="control-button"
            onClick={() => {
              void onStop();
            }}
          >
            Stop
          </button>
          <button
            type="button"
            className="control-button"
            onClick={() => {
              void onTogglePause();
            }}
          >
            {pauseLabel}
          </button>
          <button
            type="button"
            className="control-button"
            onClick={() => {
              void onClear();
            }}
          >
            Clear
          </button>
        </div>

        <div className="overlay-footer__group">
          <button
            type="button"
            className="control-button"
            onClick={() => {
              void onSettings();
            }}
          >
            Settings
          </button>
        </div>
      </footer>
    </section>
  );
}
