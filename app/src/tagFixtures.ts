import type { Tag } from "./managedClips";

export const customTag = (label: string): Tag => ({ id: `test:${label}`, label, builtin: false, tier: null });
export const pentakill: Tag = { id: "builtin:pentakill", label: "Pentakill", builtin: true, tier: 5 };
