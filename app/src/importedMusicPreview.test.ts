import { expect, it, vi } from "vitest";
import { createImportedMusicPreview } from "./importedMusicPreview";

it("revokes a stale pending import before preparing its replacement", async () => {
  let resolve!: (value: {url: string; token: string}) => void;
  const prepare = vi.fn().mockImplementationOnce(() => new Promise((done) => { resolve = done; }))
    .mockResolvedValueOnce({url: "new-url", token: "new"});
  const release = vi.fn().mockResolvedValue(undefined);
  const changed = vi.fn();
  const failed = vi.fn();
  const preview = createImportedMusicPreview({prepare, release, changed, failed});
  preview.select("old.wav");
  await vi.waitFor(() => expect(prepare).toHaveBeenCalledTimes(1));
  preview.select("new.wav");
  resolve({url: "old-url", token: "old"});
  await preview.settled();
  expect(release).toHaveBeenCalledWith("old");
  expect(changed).not.toHaveBeenCalledWith("old-url");
  expect(changed).toHaveBeenLastCalledWith("new-url");
  expect(failed).not.toHaveBeenCalled();
  preview.dispose();
  await preview.settled();
  expect(release).toHaveBeenLastCalledWith("new");
});

it("disposal revokes an import whose command finishes after unmount", async () => {
  let resolve!: (value: {url: string; token: string}) => void;
  const prepare = vi.fn(() => new Promise<{url:string;token:string}>((done) => { resolve = done; }));
  const release = vi.fn().mockResolvedValue(undefined);
  const changed = vi.fn();
  const preview = createImportedMusicPreview({prepare, release, changed, failed: vi.fn()});
  preview.select("pending.wav");
  await vi.waitFor(() => expect(prepare).toHaveBeenCalledTimes(1));
  preview.dispose();
  resolve({url:"late-url",token:"late"});
  await preview.settled();
  expect(release).toHaveBeenCalledExactlyOnceWith("late");
  expect(changed).not.toHaveBeenCalledWith("late-url");
  preview.select("ignored.wav");
  expect(prepare).toHaveBeenCalledTimes(1);
});
