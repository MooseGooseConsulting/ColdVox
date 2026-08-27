import { act, render, screen, waitFor } from "@testing-library/react";
import { useOverlayShell } from "./useOverlayShell";
import type { OverlayEvent, OverlaySnapshot } from "../contracts/overlay";
import type { UnlistenFn } from "@tauri-apps/api/event";

const bridgeMocks = vi.hoisted(() => {
  let listener: ((event: OverlayEvent) => void) | null = null;
  const idleSnapshot: OverlaySnapshot = {
    expanded: false,
    status: "idle",
    paused: false,
    partialTranscript: "",
    finalTranscript: "",
    statusDetail: "Host shell connected.",
    errorMessage: null,
  };

  return {
    idleSnapshot,
    getOverlaySnapshot: vi.fn<() => Promise<OverlaySnapshot>>().mockResolvedValue(idleSnapshot),
    setOverlayExpanded: vi
      .fn<(expanded: boolean) => Promise<OverlaySnapshot>>()
      .mockImplementation(async (expanded) => ({ ...idleSnapshot, expanded })),
    startPipeline: vi.fn<() => Promise<OverlaySnapshot>>().mockResolvedValue({
      ...idleSnapshot,
      expanded: true,
      status: "listening",
    }),
    togglePauseState: vi.fn<() => Promise<OverlaySnapshot>>().mockResolvedValue({
      ...idleSnapshot,
      expanded: true,
      status: "listening",
      paused: true,
    }),
    stopPipeline: vi.fn<() => Promise<OverlaySnapshot>>().mockResolvedValue(idleSnapshot),
    clearOverlayTranscript: vi.fn<() => Promise<OverlaySnapshot>>().mockResolvedValue(idleSnapshot),
    openSettingsPlaceholder: vi
      .fn<() => Promise<OverlaySnapshot>>()
      .mockResolvedValue({ ...idleSnapshot, expanded: true, statusDetail: "Settings later." }),
    subscribeToOverlayEvents: vi.fn<(callback: (event: OverlayEvent) => void) => Promise<UnlistenFn>>()
      .mockImplementation(async (callback) => {
        listener = callback;
        return () => {
          listener = null;
        };
      }),
    // Pipeline wiring — real STT integration
    updatePartialTranscript: vi.fn<(text: string) => Promise<OverlaySnapshot>>().mockResolvedValue({
      ...idleSnapshot,
      expanded: true,
      status: "listening",
      partialTranscript: "partial text",
    }),
    updateFinalTranscript: vi.fn<(text: string) => Promise<OverlaySnapshot>>().mockResolvedValue({
      ...idleSnapshot,
      expanded: true,
      status: "ready",
      finalTranscript: "final text",
    }),
    setOverlayProcessing: vi.fn<() => Promise<OverlaySnapshot>>().mockResolvedValue({
      ...idleSnapshot,
      expanded: true,
      status: "processing",
    }),
    setOverlayListening: vi.fn<() => Promise<OverlaySnapshot>>().mockResolvedValue({
      ...idleSnapshot,
      expanded: true,
      status: "listening",
    }),
    stopOverlayCapture: vi.fn<() => Promise<OverlaySnapshot>>().mockResolvedValue(idleSnapshot),
    emit(event: OverlayEvent) {
      listener?.(event);
    },
  };
});

vi.mock("../lib/overlayBridge", () => ({
  getOverlaySnapshot: bridgeMocks.getOverlaySnapshot,
  setOverlayExpanded: bridgeMocks.setOverlayExpanded,
  startPipeline: bridgeMocks.startPipeline,
  togglePauseState: bridgeMocks.togglePauseState,
  stopPipeline: bridgeMocks.stopPipeline,
  clearOverlayTranscript: bridgeMocks.clearOverlayTranscript,
  openSettingsPlaceholder: bridgeMocks.openSettingsPlaceholder,
  subscribeToOverlayEvents: bridgeMocks.subscribeToOverlayEvents,
  updatePartialTranscript: bridgeMocks.updatePartialTranscript,
  updateFinalTranscript: bridgeMocks.updateFinalTranscript,
  setOverlayProcessing: bridgeMocks.setOverlayProcessing,
  setOverlayListening: bridgeMocks.setOverlayListening,
  stopOverlayCapture: bridgeMocks.stopOverlayCapture,
}));

function HookHarness() {
  const { snapshot, setExpanded, startPipeline } = useOverlayShell();

  return (
    <div>
      <span data-testid="status">{snapshot.status}</span>
      <span data-testid="detail">{snapshot.statusDetail}</span>
      <span data-testid="partial">{snapshot.partialTranscript}</span>
      <button
        type="button"
        onClick={() => {
          void setExpanded(true);
        }}
      >
        expand
      </button>
      <button
        type="button"
        onClick={() => {
          void startPipeline();
        }}
      >
        demo
      </button>
    </div>
  );
}

describe("useOverlayShell", () => {
  beforeEach(() => {
    vi.clearAllMocks();
  });

  it("loads the initial snapshot and subscribes to overlay events", async () => {
    render(<HookHarness />);

    await waitFor(() => {
      expect(screen.getByTestId("detail")).toHaveTextContent("Host shell connected.");
    });

    act(() => {
      bridgeMocks.emit({
        reason: "demo-step",
        snapshot: {
          ...bridgeMocks.idleSnapshot,
          expanded: true,
          status: "processing",
          partialTranscript: "streamed partial text",
          statusDetail: "Processing demo result.",
        },
      });
    });

    expect(screen.getByTestId("status")).toHaveTextContent("processing");
    expect(screen.getByTestId("partial")).toHaveTextContent("streamed partial text");
  });

  it("routes UI actions through the bridge helpers", async () => {
    render(<HookHarness />);

    await waitFor(() => {
      expect(bridgeMocks.getOverlaySnapshot).toHaveBeenCalledTimes(1);
    });

    act(() => {
      screen.getByRole("button", { name: "expand" }).click();
      screen.getByRole("button", { name: "demo" }).click();
    });

    await waitFor(() => {
      expect(bridgeMocks.setOverlayExpanded).toHaveBeenCalledWith(true);
      expect(bridgeMocks.startPipeline).toHaveBeenCalledTimes(1);
    });
  });
});
