import type { ImportedMusicPreview } from "./api";

/** Serializes the one registered server preview and revokes stale async results. */
export const createImportedMusicPreview = (ports: {
  prepare: (path: string) => Promise<ImportedMusicPreview>;
  release: (token: string) => Promise<void>;
  changed: (url: string) => void;
  failed: (error: unknown) => void;
}) => {
  let generation = 0;
  let current: ImportedMusicPreview | undefined;
  let work = Promise.resolve();
  let disposed = false;

  const release = async (preview: ImportedMusicPreview | undefined) => {
    if (preview) await ports.release(preview.token);
  };
  const select = (path: string) => {
    if (disposed) return;
    const selectedGeneration = ++generation;
    ports.changed("");
    work = work.then(async () => {
      const previous = current;
      current = undefined;
      await release(previous);
      if (disposed || selectedGeneration !== generation || !path) return;
      const preview = await ports.prepare(path);
      if (disposed || selectedGeneration !== generation) {
        await release(preview);
      } else {
        current = preview;
        ports.changed(preview.url);
      }
    }).catch((error: unknown) => {
      if (!disposed && selectedGeneration === generation) ports.failed(error);
    });
  };
  return {
    select,
    dispose: () => {
      if (disposed) return;
      select("");
      disposed = true;
    },
    settled: () => work,
  };
};
