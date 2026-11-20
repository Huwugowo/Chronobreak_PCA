import type { Tag, TagAssignment } from "./managedClips";

export const customTag = (label: string): Tag => ({ id: `test:${label}`, label, builtin: false, tier: null });
export const pentakill: Tag = { id: "builtin:pentakill", label: "Pentakill", builtin: true, tier: 5 };
export const assigned = (tag: Tag, source: TagAssignment["source"] = "manual"): TagAssignment => ({ ...tag, source });
