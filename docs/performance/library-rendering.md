# Match History offscreen rendering

QB-LIB-017, measured on Windows on 2026-10-21. Every existing card image already
had native lazy loading and asynchronous decoding; this pass does not introduce
or claim additional image-request deferral.

## Change

Match cards use `content-visibility: auto` with a remembered intrinsic block-size
estimate matching the existing 109px minimum outer height. The browser can skip
distant card layout/paint while retaining the complete DOM, text search, focus,
actions and existing image-loading policy. No list virtualization, mount/unmount
controller or dependency was added. Browsers that do not support this CSS retain
the existing full rendering behavior.

## Reproduction

Start the Vite development server on port 5173, then run:

```powershell
npm.cmd run dev --prefix app -- --port 5173
node tools/library_benchmark/measure.mjs "<absolute Playwright module path>" "<absolute Chromium executable path>"
```

The opt-in `app/library-benchmark.html` entry mounts the real LibraryScreen with
50 deterministic checked rows, full item/spell/rune slots and ten participants.
It uses bundled mock assets and no backend or user library. The fixture is not
imported by the production entry and rejects non-development execution.
`?baseline` disables only the new content visibility rule for comparison.
The runner creates a fresh browser context for each of three before/after runs.

The browser was installed Edge 155.0.4283.62, headless, 1280×800. Module compilation
was warmed before measurement. The render milestone spans component mount to
the second animation-frame callback. It is not application startup or physical
presentation latency. Initial metrics wait for first-card image completion or
the existing unavailable-image fallback. Reading every image's bounding box was
deliberately excluded because it forces offscreen layout and invalidates the
experiment. Browser `contentvisibilityautostatechange` events report deferred
cards; the visibility-check API did not expose those states reliably here.

## Results

| Metric | Baseline | Deferred rendering |
| --- | --- | --- |
| Render milestone, three runs (ms) | 177.2 / 138.1 / 141.4 | 110.7 / 119.0 / 110.4 |
| Median render milestone (ms) | 141.4 | 110.7 |
| Cards deferred by browser initially | 0 | 33 |
| Initial layout duration (ms) | 58.725 / 51.167 / 54.598 | 49.569 / 55.686 / 51.838 |
| Initial style duration (ms) | 16.886 / 16.309 / 16.734 | 24.879 / 24.910 / 23.735 |
| Scripted scroll frame gap p95 (ms) | 18.0 / 18.0 / 18.0 | 18.1 / 18.0 / 18.0 |
| Scripted scroll maximum gap (ms) | 106.5 / 18.3 / 18.2 | 18.2 / 18.2 / 18.0 |

Both variants retain 50 cards, 1,092 image elements, 353 initially loaded images
and a 5,973px page height. The baseline adds one diagnostic style node; otherwise
the DOM size is unchanged (5,207 elements). The median render milestone improved
by 21.7%; style calculation increased and initial layout duration changed little.
This is a modest local rendering result, not a CPU, memory, GPU or general
scrolling-speed claim. The isolated baseline scroll spike is retained rather than
interpreted as a proven defect fixed by the change. Three sequential development
browser repetitions do not establish production WebView tail latency.

## Interaction verification

At 1280×800, focusing the last offscreen card and pressing Enter invoked the
correct recording action and scrolled it into view. Its images loaded on reveal
(21 available images), while the unavailable asset retained the existing fallback.
The last-card menu remained inside the card (menu y=701.5–741.5, card
y=667–776); its delete action invoked only the fixture callback. No media is
modified. Filtering Syndra returned nine cards; clearing restored all 50.
At 1000×700, the last card remained 109px high and its menu was visible.
The page height stayed 5,973px after scrolling/focus. Screenshots were inspected.
Existing horizontal scrolling at narrow widths remains a separate layout issue.

All 279 frontend tests across 31 files, TypeScript, Vite and the fresh Tauri
production build passed. Canonical validation and diff whitespace checks passed.
The fixture remains outside the production bundle. Local raw comparison
and screenshots are under ignored `build/perf/qb-lib-017/`.
