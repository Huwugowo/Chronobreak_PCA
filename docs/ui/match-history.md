# Chronobreak Match History UI Spec

**Status:** Canonical screen contract
**Revision:** 3
**Depends on:** `docs/ui/chronobreak-ui-foundations.md`

## 1. Purpose

This document records **Match History-specific** UX/presentation decisions worth preserving as the screen evolves.

It does not define backend behavior, recording behavior, or playback architecture.

When this document is silent, the global UI foundations apply. Silence means the choice has not been made yet; do not promote a one-off implementation detail into a durable screen rule without deciding it explicitly.

## 2. Product role

Visible screen name: **Match History**.

Do not present the screen as:
- Games
- Game Library
- Match Archive

Internal identifiers may remain `games` / `Library*`; visible naming does not require an internal rename.

The screen's job is to let the user:
- scan recent recorded matches quickly;
- recognize the match they want;
- understand key match/recording metadata;
- open a replay with minimal friction;
- save or manage a recording.

## 3. Structural direction

The League client match-history list is the **structural baseline, not the visual skin**.

Use the same broad information architecture: dense horizontal match rows, familiar grouping of champion/loadout/items/KDA/match metadata, and fast vertical scanning. Chronobreak's typography, surfaces, spacing, controls, color semantics, and chrome remain its own.

Do not add a large hero/intro panel.

Recording/saved counters are not part of the current screen contract. If a later product need justifies them, treat that as a new screen decision rather than a default dashboard pattern.

Do not add search/filter/sort controls until the corresponding product behavior actually exists. When that behavior becomes a task, design the controls then; do not preserve speculative fake controls merely because an earlier mockup contained them.

## 4. Match row contract

Default row height: `72px`.

Default content order:

1. champion portrait
2. result when authoritative + current authoritative mode label
3. summoner spells + keystone
4. final items
5. K / D / A + KDA ratio
6. duration
7. recording size
8. date/time
9. independent utility controls: Star when save is eligible; More when at least one additional action is available

There is **no visible Open button**.

A final-level badge is not part of the current row contract. Adding one later is a new screen decision and requires authoritative data.

### 4.1 Dimensions

```css
:root {
  --match-row-height: 72px;
  --match-row-padding-x: 12px;
  --match-row-gap: 12px;

  --champion-icon-size: 44px;
  --spell-icon-size: 20px;
  --rune-icon-size: 20px;
  --item-icon-size: 24px;
}
```

Do not reduce row height below roughly `64px` without validating actual Windows readability and target sizes.

Use explicit column alignment. Compress gaps before shrinking typography.

## 5. Champion identity

- Champion portrait: `44×44px`.
- Circular portrait treatment is allowed.
- **Do not visibly display the champion name in the normal Match History row.**
- Keep champion identity available programmatically for accessible labeling.
- Missing champion art must preserve row geometry and use a quiet fallback rather than broken-image UI.

Adding visible champion names later is a product/usability decision, not an optical tweak.

## 6. Result and replay health

Match outcome and replay health are independent dimensions.

### 6.1 Outcome

When authoritative outcome exists:

- `VICTORY` → global blue accent
- `DEFEAT` → global red accent
- `REMAKE`, `TERMINATED`, equivalent non-win/loss → global neutral accent

Keep the explicit word; color is supplementary.

If no authoritative outcome is available in the real application, **omit the outcome**. The UI must not infer it from K/D/A, duration, final events, or other heuristics.

A backend/data layer may eventually expose an authoritative outcome derived through its own validated contract; that is different from the presentation layer guessing one.

### 6.2 Replay health

Current replay-health facts:
- `INCOMPLETE`
- `UNAVAILABLE`

They describe different conditions:
- `INCOMPLETE` → match/recording metadata or capture is incomplete/recovered.
- `UNAVAILABLE` → the replay video cannot currently be opened.

Neither replaces match outcome.

The two health facts are not logically mutually exclusive. If both apply, the UI must preserve both meanings; presentation may compact them rather than pretending one condition does not exist.

Do not label a recording `ERROR` unless an authoritative error condition exists.

## 7. Mode / queue identity

Use only metadata the product actually knows.

If only `game_mode` is authoritative, show that value or a faithful presentation of it.

If mode metadata is absent/unknown, use a neutral missing-value presentation such as `—`. Do not use a replay-health phrase such as “Recovered recording” as if it were a queue/mode.

Do not silently invent user-facing queue identities such as:
- Ranked Solo/Duo
- Normal Draft

until the data contract actually distinguishes those concepts.

## 8. Spells, rune, and items

Preserve League-provided artwork/color.

- spells: `20×20px`
- keystone: `20×20px`
- items: `24×24px`
- fixed item slots preserve row alignment
- missing assets keep their slots and remain visually quiet

Do not add decorative frames unless they solve a real recognition/alignment need.

## 9. K/D/A and numeric metadata

Use the global Spiegel/data typography role.

- Do not use monospace by default.
- Keep K/D/A and ratio visually grouped.
- Duration and recording size align consistently across rows.
- Recording size remains visible as a Chronobreak-specific field.
- Date/time should be compact and locale-appropriate.
- Missing scalar values may use `—`.

Deterministic formatting or arithmetic derived directly from authoritative fields is allowed. For example, formatting duration/size/date or calculating the defined KDA ratio is not the same as inventing a missing semantic fact.

Incomplete recordings may omit derived KDA values rather than presenting misleading precision.

## 10. Open interaction

Opening a replay should require one ordinary left click anywhere in the row's **primary content area**.

The primary content area includes:
- champion
- result/mode
- spells/rune
- items
- KDA
- duration
- size
- date/time
- otherwise unused primary-content space

Independent controls are excluded:
- Star when present
- More when present
- any future explicit row utility

Those controls must not also trigger replay opening.

Implementation should use:
- one large focusable replay-open target for ordinary row content;
- separate sibling controls for Star/More.

Do not nest interactive controls inside another button.

If the replay video is unavailable:
- the open action is disabled/non-activatable;
- the row's metadata remains readable;
- unavailable semantics remain exposed accessibly;
- the whole row should not be visually washed out as if all information were disabled.

## 11. Saved state

Saved state is represented by **Star only**.

- unsaved: outline star
- saved: filled star
- saved star may use Chronobreak yellow

Do not add:
- saved row tint
- saved badge
- saved label
- saved border
- other row-layout changes

The outline/fill shape change ensures saved state does not depend on color alone.

The Star appears only when the recording is eligible for the existing save behavior. The UI must not broaden save eligibility on its own.

## 12. More / destructive actions

More is a neutral icon-only utility control and appears only when at least one eligible secondary action exists.

Destructive operations such as Delete live behind the More action / confirmation flow.

- More itself is not red.
- Actual Delete action is red.
- Confirmation remains explicit.
- Do not tint the row red merely because delete exists.

## 13. Row states

### Rest
- quiet surface
- thin structural separator

### Hover
- neutral `--surface-hover`
- no translation/scale
- no glow
- no semantic-color wash

### Keyboard focus
- the focused open/control target receives the global focus treatment

### Saved
- filled yellow Star only

### Outcome
- result label color only; do not tint the full row

### Busy mutation
- disable only the control(s) that must not repeat
- do not dim/block the entire Match History screen
- no full-screen spinner for ordinary short save/delete operations

## 14. Empty and degraded states

### Empty
- no hero panel
- one quiet empty state in the content/list area
- short message such as `No recordings yet`
- brief explanation that future recordings appear here
- no oversized illustration or branding

### Incomplete/recovered
- same row geometry as normal recordings
- explicit `INCOMPLETE` status
- unavailable derived values may be `—`
- preserve legitimate current actions

Do not call the bundle corrupted unless the application actually knows that.

### Video unavailable
- keep metadata readable
- disable replay opening
- explicit neutral `UNAVAILABLE` meaning
- provide accessible explanation where practical

### Missing Data Dragon artwork
- keep row geometry
- champion fallback may use `?`
- spell/rune/item slots stay stable
- no repeated per-row global-network warning
- no broken browser-image UI

### Error scope

Show problems at the narrowest useful scope:
1. field/asset fallback
2. row status
3. screen/application notice only when broadly relevant or actionable

## 15. Accessibility

- Champion name remains available in the replay-open accessible label despite being visually hidden.
- Star and More require accessible names.
- The replay-open target requires keyboard access when activation is available.
- Unavailable replay state must remain accessibly communicated even if the normal open target is non-activatable.
- Result/health/saved states may not rely on color alone.
- Hit targets obey the global minimum.

If visual column headings are shown, treat them as scanning aids; expose them to assistive technology only when doing so adds useful structure rather than repetitive noise.

## 16. Width behavior

Chronobreak is desktop-first.

When width is constrained:
1. preserve champion
2. preserve result/mode
3. preserve KDA
4. preserve duration
5. preserve actions
6. compress gaps and lower-priority metadata before shrinking text

Do not introduce a mobile card layout for the Windows desktop product.

Horizontal scrolling is acceptable for a dense table-like Match History if it preserves readable fixed-density data better than crushing columns.

## 17. Performance

The global performance contract applies. Match History specifically does not justify:
- virtualization without measured need
- per-row animation systems
- per-row shadows/glows
- extra reactive state for static fields
- broad icon/UI dependencies

## 18. Production data rule

Production presentation may use:
- authoritative fields supplied by the relevant data contract;
- deterministic formatting/derivation directly from those fields.

It must not infer or fabricate a missing **semantic fact**.

The outcome presentation defined in §6 is intentional, but the real application must omit it until the summary data contract exposes an authoritative outcome.

The current row contract does **not** require:
- final player level
- richer queue identity than the authoritative mode metadata already available
- speculative replay-health/error categories beyond states the application actually knows

If any of those concepts are introduced later, the relevant product/data contract must support them first.

Development/test fixtures may exercise states that production data does not yet expose, but fixture mechanics are not part of this canonical design contract.

## 19. Change discipline

Update this screen spec when a Match History UX/presentation decision materially changes.

Do not update it for:
- tiny optical spacing corrections inside the global rules
- migration mechanics
- current patch filenames
- temporary preview data
- current implementation progress

If a Match History pattern later becomes a shared application pattern, promote it into `chronobreak-ui-foundations.md` and reference the global rule here instead of duplicating it.
