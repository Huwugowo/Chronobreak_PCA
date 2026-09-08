import { afterEach, describe, expect, it, vi } from "vitest";
import type { PlaybackSnapshot } from "./playbackController";
import type { MediaTimelineV2 } from "./replayTime";
import { createPlaybackDiagnostics, type DecoderSnapshot } from "./playbackDiagnostics";

const mock = vi.hoisted(() => ({ invoke: vi.fn(), listen: vi.fn(), remove: vi.fn() }));
vi.mock("@tauri-apps/api/core", () => ({ invoke: mock.invoke, isTauri: () => true }));
vi.mock("@tauri-apps/api/event", () => ({ listen: mock.listen }));
afterEach(() => vi.resetAllMocks());

describe("decoder bridge lifetime", () => {
  it("ignores stale-generation replies and removes the native owner on disposal", async () => {
    const pending: Array<(value: DecoderSnapshot) => void> = [];
    let listener!: (event: { payload: DecoderSnapshot }) => void;
    let owner = "";
    mock.listen.mockImplementation(async (_kind, callback) => { listener = callback; return mock.remove; });
    mock.invoke.mockImplementation((command, args) => {
      owner = args.owner;
      if (command === "open_playback_diagnostics") return Promise.resolve({ enabled: true });
      if (command === "bind_playback_diagnostics") return new Promise((resolve) => pending.push(resolve));
      return Promise.resolve();
    });
    const changes: DecoderSnapshot[] = [];
    const bridge = createPlaybackDiagnostics((value) => changes.push(value));
    await bridge.ready;
    const snapshot = (sessionToken: string, generation: number) => ({ sessionToken, generation,
      mediaId: "media", sourceUrl: "http://127.0.0.1/video" }) as PlaybackSnapshot;
    const timeline = { video: { codec: "h264", profile: "High" } } as MediaTimelineV2;
    bridge.bind(snapshot("old", 1), timeline); await Promise.resolve();
    bridge.bind(snapshot("new", 2), timeline); await Promise.resolve();
    const evidence = (session: string, generation: number) => ({ owner, session_token: session,
      media_id: "media", generation, status: "hardware-confirmed" }) as DecoderSnapshot;
    pending[0](evidence("old", 1)); await Promise.resolve();
    expect(changes.some((value) => value.status === "hardware-confirmed")).toBe(false);
    pending[1](evidence("new", 2)); await Promise.resolve();
    expect(changes[changes.length - 1].session_token).toBe("new");
    bridge.dispose(); const count = changes.length;
    listener({ payload: evidence("new", 2) });
    expect(changes).toHaveLength(count); expect(mock.remove).toHaveBeenCalledOnce();
    expect(mock.invoke).toHaveBeenCalledWith("close_playback_diagnostics", { owner });
  });
});
