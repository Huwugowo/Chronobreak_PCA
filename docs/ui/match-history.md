# Chronobreak Match History UI Spec

**Status:** Canonical screen contract
**Revision:** 5
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

Mainstream League match histories such as OP.GG and DeepLoL are the **information-architecture baseline, not the visual skin**.

Chronobreak uses a vertically stacked list of self-contained **match items**, not table rows. Each item groups related information semantically instead of assigning every value its own global column.

Normal desktop reading order:

**match context -> player summary/build -> both team rosters -> recording metadata -> actions**

Each match is an independent object with its own surface, border, hover state, spacing, and outcome cue.

Do not treat Match History as a table:

- no shared borders between adjacent matches;
- no one-grid-track-per-field layout;
- no zero-gap row stack;
- no table header;
- no requirement that unrelated scalar values align to the same x-coordinate;
- no visual dependence between adjacent matches.

The toolbar remains compact and supports search, Champion filtering, Mode filtering, Favorites-only filtering, and quiet recording/saved counters.

## 4. Match item contract

Default complete match-item minimum height: `109px`.

Default semantic grouping:

1. match context: authoritative mode/queue identity, relative age, outcome/health, duration;
2. champion portrait + summoner spells + keystone;
3. K / D / A + KDA ratio + final build;
4. both five-player rosters;
5. Chronobreak recording metadata such as recording size;
6. independent Star / More actions.

There is **no visible Open button**.

The primary content area is one large replay-open target. Star and More are separate sibling controls and must not trigger replay opening.

A final-level badge is not part of the current item contract.

### 4.1 Dimensions and spacing

Baseline:

- match item minimum height: `109px`
- gap between match items: `8px`
- vertical content padding: `4px`
- card radius: `4px`
- champion portrait: `52px`
- summoner spells: `22px`
- keystone: `22px`
- item icons: `24px`
- leading outcome cue: `5px`
- match-context width: approximately `108px`
- actions remain reserved at the right edge without forming a separate card column

The approximately `109px` height is intentional. It is driven largely by the five-player roster stacks and follows the density of mainstream League match histories rather than adding decorative vertical whitespace.

Use larger spacing **between semantic groups** and smaller spacing **within groups**.

Incomplete/degraded matches preserve the same overall geometry where practical.

### 4.2 Team rosters

Complete match items display both participant teams.

Each roster:

- displays up to five participants supplied by the replay summary;
- uses `18px` champion icons;
- uses five compact rows with approximately `2px` inter-row gaps.

The two rosters form one compact cluster rather than separate table columns.

The roster cluster may occupy up to roughly `340px` and should remain adjacent to the player/build information instead of being pushed to an arbitrary far-right edge.

Do not infer local-player or local-team identity when the data contract does not establish it reliably.

## 5. Champion identity

- Champion portrait: `52×52px`.
- Circular portrait treatment is allowed.
- **Do not visibly display the champion name in the normal Match History item.**
- Keep champion identity available programmatically for accessible labeling.
- Missing champion art must preserve item geometry and use a quiet fallback rather than broken-image UI.

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

- spells: `22x22px`
- keystone: `22x22px`
- items: `24x24px`
- seven fixed item slots preserve build geometry
- item gaps: `2px`
- missing assets keep their slots and remain visually quiet

The full seven-slot item strip is `180px` wide:

`7 * 24px + 6 * 2px = 180px`

Do not add decorative frames unless they solve a real recognition/alignment need.

## 9. K/D/A and numeric metadata

Use the global Spiegel/data typography role.

- Do not use monospace by default.
- Keep K/D/A and ratio visually grouped.
- Center the K/D/A block over the `180px` item-strip footprint beneath it, not over arbitrary spare parent width.
- Duration stays grouped with match context.
- Recording size remains secondary and right-aligned.
- Missing scalar values may use `—`.

Deterministic formatting or arithmetic derived directly from authoritative fields is allowed. For example, formatting duration/size/date or calculating the defined KDA ratio is not the same as inventing a missing semantic fact.

Duration uses compact match-history formatting such as `37m 52s`.

Relative match age is shown in English, independent of the Windows/browser locale, for example `2 months ago`. The exact recorded timestamp may remain available as secondary detail such as a tooltip.

Incomplete recordings may omit derived KDA values rather than presenting misleading precision.

## 10. Open interaction

Opening a replay should require one ordinary left click anywhere in the item's **primary content area**.

The primary content area includes:
- champion
- result/mode
- spells/rune
- items
- KDA
- duration
- size
- relative match age
- otherwise unused primary-content space

Independent controls are excluded:
- Star when present
- More when present
- any future explicit item utility

Those controls must not also trigger replay opening.

Implementation should use:
- one large focusable replay-open target for ordinary item content;
- separate sibling controls for Star/More.

Do not nest interactive controls inside another button.

If the replay video is unavailable:
- the open action is disabled/non-activatable;
- the item's metadata remains readable;
- unavailable semantics remain exposed accessibly;
- the whole item should not be visually washed out as if all information were disabled.

## 11. Saved state

Saved state is represented by **Star only**.

- unsaved: outline star
- saved: filled star
- saved star may use Chronobreak yellow

Do not add:

- saved item tint
- saved badge
- saved label
- saved border
- other item-layout changes

The outline/fill shape change ensures saved state does not depend on color alone.

The Star appears only when the recording is eligible for the existing save behavior. The UI must not broaden save eligibility on its own.

Save/unsave interaction is optimistic:

- update the Star immediately in local frontend state;
- persist the new value asynchronously;
- retain the immediate Star during the successful mutation's compulsory snapshot refresh, then use the filesystem-derived value;
- roll back the local Star state if persistence fails;
- prevent overlapping save mutations for the same recording;
- discard the display overlay on unrelated refresh, navigation or library-root change.

The tokenized mutation contract requires reconciliation after every admitted
attempt, including failures. Optimistic feedback changes only display state; it
does not replace this refresh or modify the canonical snapshot.

## 12. More / destructive actions

More is a neutral icon-only utility control and appears only when at least one eligible secondary action exists.

Destructive operations such as Delete live behind the More action / confirmation flow.

- More itself is not red.
- Actual Delete action is red.
- Confirmation remains explicit.
- Do not tint the item red merely because delete exists.

## 13. Match item states

### Rest

- quiet surface
- thin `1px` structural card border
- subtle `4px` card radius

### Hover

- quiet neutral surface change
- no translation/scale
- no glow

### Keyboard focus

- compact controls use the global focus treatment
- the replay-open target may use the same restrained surface treatment as hover instead of an item-spanning outline

### Saved

- filled yellow Star only

### Outcome

Outcome treatment combines:

- the explicit result word when authoritative;
- a restrained full-card outcome tint;
- a `5px` leading semantic strip.

The current win/loss surface tint is intentionally subtle, approximately a `7%` mix of the semantic accent into the normal panel surface.

The leading strip is a separate overlay layer. It visually covers the normal `1px` card border rather than terminating against it.

Its geometry is:

- full visual item height;
- straight inner edge;
- rounded only on the outer left corners;
- visually above the ordinary card border;
- independent from content layout.

Do **not** implement the strip as an inset shadow or as a structural `border-left` that changes content geometry.

### Busy mutation

- disable only controls that genuinely must not repeat
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
- same item geometry as normal recordings
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
- keep item geometry
- champion fallback may use `?`
- spell/rune/item slots stay stable
- no repeated per-item global-network warning
- no broken browser-image UI

### Error scope

Show problems at the narrowest useful scope:
1. field/asset fallback
2. item status
3. screen/application notice only when broadly relevant or actionable

## 15. Accessibility

- Champion name remains available in the replay-open accessible label despite being visually hidden.
- Star and More require accessible names.
- The replay-open target requires keyboard access when activation is available.
- Unavailable replay state must remain accessibly communicated even if the normal open target is non-activatable.
- Result/health/saved states may not rely on color alone.
- Hit targets obey the global minimum.

Do not add table-style visual column headings to the normal Match History list.

## 16. Width behavior

Chronobreak is desktop-first.

Match History uses a bounded content rail rather than stretching match items across the entire application window.

Canonical rail behavior:

- width: `100%`
- maximum width: `1120px`
- horizontally centered

At large/fullscreen window sizes, the Match History rail therefore remains approximately `1120px` wide while surrounding application canvas grows.

At narrower supported window sizes:

- the rail shrinks with the available content width;
- the match list uses `min-width: 0`;
- do not retain a historical hard minimum such as `1040px`;
- do not introduce horizontal scrolling merely to preserve an oversized fixed layout;
- preserve icon/text readability by allowing semantic groups to use the available grid space.

This bounded-width rule is specific to Match History. Replay/viewer screens may use the viewport much more aggressively.

Do not introduce a separate mobile-card anatomy for the Windows desktop product as part of this contract.

## 17. Performance

The global performance contract applies. Match History specifically does not justify:
- virtualization without measured need
- per-item animation systems
- per-item shadows/glows
- extra reactive state for static fields
- broad icon/UI dependencies

## 18. Production data rule

Production presentation may use:
- authoritative fields supplied by the relevant data contract;
- deterministic formatting/derivation directly from those fields.

It must not infer or fabricate a missing **semantic fact**.

The outcome presentation defined in §6 is intentional, but the real application must omit it until the summary data contract exposes an authoritative outcome.

The current match-item contract does **not** require:
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
