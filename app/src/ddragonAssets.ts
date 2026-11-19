import type { DdragonStatus } from "./types";

export const ddragonAsset = (
  status: DdragonStatus,
  kind: "champion" | "spell" | "rune" | "item",
  asset: string | number | null | undefined,
): string | null => {
  if (!status.asset_base_url || asset === null || asset === undefined || asset === "") return null;
  return `${status.asset_base_url}/${kind}/${encodeURIComponent(String(asset))}`;
};

export const hideFailedImage = (event: Event) => {
  if (event.currentTarget instanceof HTMLImageElement) event.currentTarget.hidden = true;
};
