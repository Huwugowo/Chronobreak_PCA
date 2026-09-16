# Chronobreak UI Foundations

**Status:** Canonical
**Revision:** 3
**Scope:** Cross-screen visual, interaction, accessibility, and UI-performance rules.

## 1. Purpose and authority

This document is Chronobreak's long-term source of truth for **shared UI design decisions**.

“Canonical” means **the current intended design**, not immutable history. A decision can change, but an intentional change to a documented rule should update this document together with the affected implementation/specs.

It is authoritative for:
- product visual character
- shared palette and color-role policy
- typography roles
- geometry and density
- shared interaction/focus/motion rules
- icon policy
- accessibility floor
- UI performance constraints
- app-shell visual/product identity

It is **not** authoritative for:
- playback/media-timing architecture
- recording/capture behavior
- storage/business logic
- backend data contracts
- temporary development fixtures
- migration mechanics
- current implementation status
- patch/branch/commit instructions
- screen layouts that have not yet been designed

Screen-specific decisions belong in `docs/ui/<screen>.md`.

### 1.1 Source-of-truth precedence

- This document governs cross-screen UI rules.
- A screen spec may refine these rules but must not silently contradict them.
- Behavioral/backend/playback specifications remain authoritative for their own domains.
- Code is authoritative for what is currently implemented; these UI documents are authoritative for intended UI design.
- An intentional design change requires the relevant canonical UI document to change with it.

### 1.2 How to interpret silence and defaults

Do not turn this document into a backlog of undecided future design.

- **Do not / must not** expresses an invariant unless the design decision is explicitly changed.
- **Default / prefer / should** expresses the expected design direction; a screen may deviate for a concrete reason.
- **May** means permitted, not required.
- If this document and the relevant screen spec are silent, the choice is **undecided**. Decide it when a real screen/product need requires it.
- A one-screen choice does not become a global rule merely because it exists in code.

Promote a screen decision into this document only when it is cross-screen, protects product identity/performance/accessibility, defines shared semantics, or is likely to drift if left implicit.

## 2. Product UI character

Chronobreak should feel:

- **fast**
- **precise**
- **quiet**
- **dense**
- **technical**
- **desktop-native**
- **slightly competitive**

The visual metaphor is a high-performance replay instrument, not a decorative gaming dashboard.

Chronobreak should **not** feel like:

- a generic SaaS dashboard
- an imitation of the League client
- an RGB / neon gaming launcher
- a glassmorphism-heavy app
- a general-purpose video editor

These are rejection criteria, not mood-board language. A proposed direction that materially pushes the product toward one of those categories needs an explicit product reason.

## 3. Content-first visual principle

Chronobreak is a restrained frame around League content.

Game-provided content may carry most of the screen's color:
- champions
- items
- summoner spells
- runes
- thumbnails
- replay video

Application chrome should not compete with that content.

Hierarchy should come primarily from:
1. layout and spacing
2. luminance/surface differences
3. typography
4. thin structural lines
5. semantic color only where it carries meaning

Avoid decorative color cycling, ornamental glow, and large colorful surfaces.

## 4. Performance contract

The UI is part of Chronobreak's performance-first product promise.

Prefer:
- the existing frontend stack and platform/browser primitives
- semantic HTML
- CSS custom properties and scoped styles
- small reusable components when repetition justifies them
- stable DOM structure
- narrow reactive updates
- curated local SVG assets

Do not change frontend framework or add a UI framework solely to achieve visual styling that the existing stack can express cleanly.

Avoid unless measured/justified:
- large component libraries
- broad runtime icon packages
- animation frameworks for ordinary interaction
- backdrop blur / glassmorphism
- continuous decorative animation
- decorative Canvas/WebGL
- large shadows/glows over broad regions
- unnecessary extra video surfaces
- speculative virtualization
- excessive reactive state for static presentation

Do not micro-optimize ordinary static CSS. Escalate choices that may materially affect:
- startup time
- bundle size
- CPU/GPU use
- RAM/VRAM
- replay responsiveness

UI work must not casually alter playback, capture, media timing, storage, or backend behavior to achieve a visual result.

## 5. Foundation palette

```css
:root {
  --surface-app: #0D0C10;
  --surface-panel: #14121A;
  --surface-raised: #1B1822;
  --surface-hover: #231F2C;
  --surface-pressed: #100F14;

  --brand-plum: #48405C;
  --brand-yellow: #FAFA69;

  --text-primary: #F5F3F7;
  --text-secondary: #B7B2C0;
  --text-muted: #928B9B;

  --accent-blue: #4DA3FF;
  --accent-red: #FF5268;
  --accent-neutral: #9CA0AA;

  --line-subtle: rgba(245, 243, 247, 0.12);
  --line-strong: rgba(245, 243, 247, 0.24);

  --focus-ring: var(--text-muted);

  --selected-surface: rgba(250, 250, 105, 0.06);
  --selected-line: rgba(250, 250, 105, 0.55);

  --disabled-opacity: 0.45;
}
```

### 5.1 Brand roles

- Plum is a brand anchor/structural color, not the full application background.
- Yellow is Chronobreak's active/selection accent.
- Yellow should remain sparse enough to retain hierarchy.

### 5.2 Semantic hue policy

The three non-brand accent colors are shared **palette primitives**, not interchangeable semantic tokens.

Current hue policy:
- blue family → victory / allied semantics
- red family → defeat / enemy / destructive semantics
- neutral grey family → neutral/non-standard/degraded semantics

When code needs a semantic token, prefer a context-specific alias such as a result, relation, or destructive-action token. Do not couple unrelated concepts by making one component reuse another component's semantic token merely because their colors currently match.

Do not use blue/red as generic hover or decoration.

Green is not part of the normal interaction palette. If a genuine positive/success/ready semantic is later required, define it for that real use case rather than reserving a speculative token now.

Do not use Chronobreak yellow as warning merely because it is bright. If a real warning system is required, define that semantic family for the actual use case.

### 5.3 Color is supplementary

State must not rely on color alone.

Use explicit text, icon shape, structural cue, or another non-color signal for:
- semantic result/status
- saved state
- focus
- selected state
- disabled state
- destructive actions
- degraded/error states

## 6. Typography

### 6.1 Roles

Design direction:
- **Spiegel** for ordinary UI/body/data text.
- **Beaufort for LoL** selectively for short display/result treatment.
- No dedicated monospace visual language by default.

Spiegel is appropriate for:
- navigation
- controls
- settings
- match metadata
- K/D/A
- durations
- recording sizes
- dates
- helper text

Beaufort is appropriate for:
- short section/display titles
- result labels
- restrained brand/display accents

Do not use Beaufort for dense body copy.

Do not introduce monospace merely to make the product look technical. Use it only if real alignment/readability needs justify it.

### 6.2 Type scale

| Role | Font | Size | Weight | Line height |
|---|---|---:|---:|---:|
| display | Beaufort | 18px | 700 | 22px |
| result | Beaufort | 14px | 700 | 18px |
| UI strong | Spiegel | 14px | 600 | 20px |
| data | Spiegel | 14px | 500 | 20px |
| secondary | Spiegel | 13px | 400 | 18px |
| compact label | Spiegel | 11px | 500 | 14px |
| low-priority meta | Spiegel | 10px | 400 | 14px |

Rules:
- Prefer 11px or larger for persistent labels.
- 10px is reserved for genuinely low-priority metadata.
- Do not use visible UI text below 10px.
- Avoid excessive uppercase/letter spacing; uppercase is for short semantic/display labels, not ordinary metadata.
- Small optical adjustments do not require a design-system revision if the role/hierarchy is unchanged.

### 6.3 Font distribution gate

The typography roles are canonical; the exact Riot font files are not approved for distribution yet.

Before shipping:
- verify third-party redistribution/embedding rights;
- bundle only the exact permitted weights/files used;
- prefer local WOFF2 where appropriate;
- do not fetch fonts remotely at runtime.

If the Riot fonts cannot legally be redistributed, preserve the same roles/hierarchy with suitable distributable substitutes rather than redesigning the interface.

## 7. Geometry and density

Chronobreak chrome is square-ish and engineered rather than soft/card-like.

Defaults:
- major panels/containers: `0px`
- list/table rows: `0px`
- ordinary controls: `2px`
- compact icon buttons: `2px`
- dialogs/popovers: `2px`
- badges/tags: `2px`
- structural lines: `1px`

Game content is exempt where its native/familiar shape improves recognition; champion portraits may be circular.

Avoid large rounded cards and pill-shaped controls as a default design language.

### 7.1 Spacing

```css
:root {
  --space-1: 4px;
  --space-2: 8px;
  --space-3: 12px;
  --space-4: 16px;
  --space-5: 24px;
  --space-6: 32px;

  --app-header-height: 56px;
  --control-height: 32px;
}
```

Use the 4px rhythm by default. One-off optical values are acceptable when they solve a real alignment problem; do not turn the scale into dogma.

Density should come from aligned information, compact grouping, restrained padding, and removal of decorative chrome.

Density must not come from unreadably small text, tiny hit targets, crushed game artwork, or excessive control stacking.

## 8. Interaction system

### 8.1 Hover

Ordinary hover:
- neutral surface/luminance change
- no glow
- no scale
- no semantic color simply because an element is hovered

### 8.2 Pressed

Pressed state may use `--surface-pressed`.

Do not animate layout/size for routine pressed feedback.

### 8.3 Focus

Default keyboard focus:

```css
:focus-visible {
  outline: 2px solid var(--focus-ring);
  outline-offset: 2px;
}
```

If the default outline is ineffective or would make a very large interactive surface read like selected state, preserve an equally obvious neutral focus indication instead. Large row-level targets may use their neutral hover surface for keyboard focus; ordinary compact controls should retain the global outline.

Do not replace focus with glow alone.

### 8.4 Selected

Selected/active UI may use:
- subtle `--selected-surface`
- a clear 1px yellow structural cue
- yellow icon/label for active top-level navigation

Do not fill ordinary active navigation with a large yellow slab.

### 8.5 Disabled

Disabled controls:
- keep geometry stable
- remove interactive hover/pressed behavior
- use the disabled opacity as a default, not as a substitute for semantics
- remain programmatically disabled or otherwise expose disabled semantics accessibly

### 8.6 Destructive

The trigger that exposes destructive actions remains neutral.

Use red at or near the actual destructive action:
- destructive menu item
- destructive confirmation
- explicit error/negative state

Do not tint an entire row red just because deletion is available.

## 9. Motion and effects

Motion is functional only.

Default transition target:
- approximately `100ms` for background/color/border state changes.

Do not use `transition: all`.

Avoid:
- continuous decorative animation
- ornamental motion
- parallax
- large blur
- backdrop-filter as default styling
- broad glow
- large animated shadows

Respect `prefers-reduced-motion`.

A loading spinner or other continuous animation is acceptable when it communicates actual ongoing work.

## 10. Icon system

**Remix Icon is Chronobreak's default source for functional UI icons.**

Policy:
- expose approved icons through Chronobreak's curated local icon layer;
- vendor only individual official SVG paths that a real implemented screen uses;
- do not ship the Remix webfont, full global stylesheet, or complete icon package;
- preserve the official 24×24 viewBox/currentColor behavior where applicable;
- do not pre-vendor speculative icons.

The implementation's curated icon module is the source of truth for the exact icon inventory.

Typical visual size:
- `14–20px`

Interactive icon controls still obey the target-size floor below.

Remix icons are functional UI assets only. Do not use a Remix icon as Chronobreak's logo, app identity, or trademark.

## 11. Accessibility floor

- Interactive hit targets: at least `24×24px`; prefer ~28–32px for ordinary compact controls.
- Keyboard focus: obvious 2px-equivalent focus indication.
- Color is never the sole carrier of meaning.
- Icon-only controls require an accessible name.
- A visual tooltip may supplement an icon-only control where useful, but must not be the sole accessible name or explanation.
- Important navigation keeps visible text labels.
- Disabled/open-unavailable states remain programmatically represented, not merely visually dimmed.
- Avoid persistent text below 11px except genuinely low-priority metadata.
- Missing/degraded content should preserve layout and provide meaningful accessible text where practical.

## 12. App-shell product identity

Visible product identity: **Chronobreak**.

Primary user destinations:
- **Match History**
- **Clips**
- **Settings**

The exact visual placement of Settings may differ from the Match History/Clips tab group; the product rule is the destination naming and hierarchy, not a particular component tree.

Do not rename internal identifiers/API/local-storage keys merely to make them match visible product labels.

The app shell should remain compact and subordinate to replay/library content.

## 13. Change discipline

This source of truth contains **decisions**, not project status or an unresolved-design backlog.

Update it when:
- a shared palette/token policy changes;
- a cross-screen interaction/accessibility rule changes;
- typography/icon/geometry strategy changes;
- a screen-specific pattern is explicitly promoted to global behavior.

Do not update it for:
- routine CSS cleanup
- isolated optical corrections inside the documented rules
- migration mechanics
- current patch/file lists
- temporary fixtures
- current implementation progress
- speculative future controls

When changing a documented rule:
1. make the design decision explicitly;
2. update this document;
3. update affected screen specs;
4. implement/migrate code deliberately.

If a future design question is not needed yet, leave it unspecified and decide it when the real task arrives.
