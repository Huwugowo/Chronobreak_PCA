// @vitest-environment jsdom
import { afterEach, expect, it, vi } from "vitest";
import { render } from "solid-js/web";
import type { AutostartStatus, RecorderStatus } from "../types";
import RecorderSettings from "./RecorderSettings";
import { setAutostart } from "../api";

const fixture = vi.hoisted(() => ({
  actual: "absent" as AutostartStatus["actual_registration"], desired: true,
  state: "available" as RecorderStatus["state"], canRetry: false,
}));
vi.mock("../api", () => ({
  loadRecorderStatus: vi.fn(async () => ({ state: fixture.state, message: "Fixture recorder state", recording_capable_here: fixture.state === "available", can_retry: fixture.canRetry })),
  retryRecorder: vi.fn(async () => {}),
  loadAutostartStatus: vi.fn(async () => ({ desired_autostart: fixture.desired, actual_registration: fixture.actual, effective_startup: "unknown", disabled_for_test: false, error: null })),
  setAutostart: vi.fn(async (enabled: boolean) => ({ desired_autostart: enabled, actual_registration: enabled ? "matching" : "absent", effective_startup: "unknown", disabled_for_test: false, error: null })),
}));
let dispose: (() => void) | undefined;
let host: HTMLDivElement;
afterEach(() => {
  dispose?.(); host?.remove(); vi.clearAllMocks();
  fixture.actual = "absent"; fixture.desired = true; fixture.state = "available"; fixture.canRetry = false;
});
function mount() {
  host = document.createElement("div"); document.body.append(host);
  dispose = render(() => <RecorderSettings />, host);
  return host;
}

it("keeps legacy desired=true with absent registration Off and sends explicit enable", async () => {
  const root = mount();
  await vi.waitFor(() => expect(root.textContent).toContain("Off — not registered"));
  const toggle = root.querySelector<HTMLInputElement>("input")!;
  expect(toggle.checked).toBe(false);
  expect(toggle.disabled).toBe(false);
  expect(setAutostart).not.toHaveBeenCalled();
  toggle.click();
  await vi.waitFor(() => expect(setAutostart).toHaveBeenCalledWith(true));
  await vi.waitFor(() => expect(toggle.checked).toBe(true));
});

it("shows matching registration On despite a false desired preference", async () => {
  fixture.actual = "matching"; fixture.desired = false;
  const root = mount();
  await vi.waitFor(() => expect(root.querySelector<HTMLInputElement>("input")?.checked).toBe(true));
  expect(root.textContent).toContain("differs from the stored preference");
  expect(setAutostart).not.toHaveBeenCalled();
});

it("disables foreign registration and offers no restart for an owner in another session", async () => {
  fixture.actual = "foreign"; fixture.state = "owner_in_other_session";
  const root = mount();
  await vi.waitFor(() => expect(root.textContent).toContain("needs attention"));
  expect(root.querySelector<HTMLInputElement>("input")?.disabled).toBe(true);
  expect(root.querySelector("button")).toBeNull();
  expect(setAutostart).not.toHaveBeenCalled();
});
