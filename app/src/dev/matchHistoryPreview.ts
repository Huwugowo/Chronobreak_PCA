import { isDesktopRuntime } from "../api";

export type PreviewMatchOutcome = "victory" | "defeat" | "remake" | "terminated";

const stableHash = (value: string): number => {
  let hash = 0x811c9dc5;
  for (let index = 0; index < value.length; index += 1) {
    hash ^= value.charCodeAt(index);
    hash = Math.imul(hash, 0x01000193);
  }
  return hash >>> 0;
};

const deterministicOutcomeFor = (timestamp: string): PreviewMatchOutcome => {
  const bucket = stableHash(timestamp) % 40;

  // Browser-preview fixtures intentionally overrepresent rare neutral states so
  // every result treatment is visible without maintaining a second fixture map.
  if (bucket >= 38) return "terminated";
  if (bucket >= 32) return "remake";
  return bucket < 16 ? "victory" : "defeat";
};

/**
 * Presentation-only outcome used by the existing browser preview.
 *
 * The Tauri runtime always returns null here, including `tauri dev`, so real
 * recordings can never be decorated with fabricated match outcomes.
 */
export const previewMatchOutcomeFor = (timestamp: string): PreviewMatchOutcome | null => {
  if (!import.meta.env.DEV || (isDesktopRuntime() && import.meta.env.VITE_UI_MOCKS !== "1")) return null;
  return deterministicOutcomeFor(timestamp);
};
