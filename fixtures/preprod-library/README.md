# Desktop preprod

The current working development library is `build/preprod/library`. On this
development machine, both normal desktop configuration and the release/dev
preprod launcher select that library. It now contains the recovered original
Master Yi MP4 and the seven newer recording bundles imported on October 26,
plus the existing Akali Practice fixture and clips. Two newer bundles are partial
captures; Akali still uses its labelled synthetic substitute. Ordinary launches
preserve this latest state.

The checked-in `fixtures/preprod-library/library` below is the **historical seed**:
real match data paired with two synthetic videos. It remains useful for frozen
regressions and first use on a fresh checkout. It does not describe the current
mutable working library, and it must never replace restored recordings.

From the repository root on integrated `main`, with Node/Rust/WebView2 and the staged locked media runtime available:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/preprod_launch.ps1
```

This builds and opens the current release executable. For normal Tauri frontend hot reload:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File tools/preprod_launch.ps1 -Dev
```

`-PrepareOnly` checks/prepares the working library without building/opening the app. Repeated launches retain recordings, saves, clips and caches. `-Reset` is disabled because the working library now contains recovered originals and added games; rebuild the disposable index through Settings when needed. A newly staged media runtime uses the existing `tools/media_runtime` setup; there is no system-FFmpeg fallback.

On first use or after rebuilding League data in Settings, allow roughly 1–2 minutes for Swiftplay's journal to finish indexing. The app imports it incrementally while you browse; subsequent opens reuse SQLite. Recorded gaps and incomplete query coverage remain visible.

All mutable data is under ignored `build/preprod`: `library`, `config.toml`, `app-data` and `webview2`. The launcher pins the output library, disables retention, clears frontend mocks and inherited benchmark activation, uses a separate WebView profile and restores its environment on exit. Keep its terminal session alive until the app closes. It refuses an unowned root, reparse paths and concurrent sessions. Existing working recordings are never reseeded. The normal local config was also routed here on 2026-10-28, so opening the release `.exe` directly uses this same recording library; launcher-specific caches/WebView isolation still require the launcher. The former `C:/Users/Hugo/LeagueReplays` files are retained as an inactive safety copy, not a second current development library.

## Historical seed recordings

| Recording | Useful UI scenarios | Known limits |
| --- | --- | --- |
| `1792431584`, Master Yi Swiftplay, queue 890 | Main dataset: ten participants, scores/items and 108 events. Minions spawn at replay 52.430140s; final Multikill at 1002.359883s. Actor filters/local timelines use app-derived unique roster game names. | Initial GameStart has no referenced calibration and remains unmapped; 107 events fall inside video. Non-player/ambiguous actors remain unresolved. |
| `1792432630`, Akali Practice Tool | Short playback, accelerated forward game clock and unavailable event mappings. | GameEnd maps to 1291.859996s, beyond the 131.916667s video. No timing repair/clamping. This source contains no backward clock drop. |

Champions/items/spells/runes use ordinary Data Dragon network/cache handling. First launch may download assets; unavailable network/assets follow the app's existing fallback. The separate browser mock preview is a different development tool.

Verified on 2026-10-20: real desktop history/event/participant filters, ten-player temporal roster, playback and forward/backward event seeks, save across restart, 13-second clip export/reopen, SQLite rebuild and normal desktop CSS hot reload. Missing source fields and unmapped observations retain the normal unavailable state. Fullscreen transitions, manual endpoint dragging and offline asset failure were not separately tested here.

## Historical seed provenance

Real sidecars come from [Huwugowo/Chronobreak_PCA, commit 7802292](https://github.com/Huwugowo/Chronobreak_PCA/commit/7802292d5fa6b9c1a59f8bcc667bd55513adc756), recorded with source `42a14671da790156d486de8c26c65908edecc593`. `source-manifest.json` preserves the archive's original manifest. The five telemetry/match/result sidecars per recording are byte-identical. `original-metadata/<id>.json` preserves each original metadata file. Identities are retained as supplied.

At seed creation on 2026-10-20, the original MP4s were unavailable. Master Yi was subsequently recovered and installed in the current working library; the immutable seed below retains its original synthetic bytes. The prepared `library/games/<id>/video.mp4` files were authored once using the locked `queueback-ffmpeg-8.1.2-windows-x86_64-r6`: libx264 High/yuv420p, 1920x1080, exact 60 FPS, timebase 1/15360, static synthetic title card, silent AAC stereo/48kHz. All video packet PTS were checked and both outputs fully decoded. Swiftplay has 60,398 frames and one-past-last PTS 15,461,888; Practice has 7,915 frames and one-past-last PTS 2,026,240. Generated audio ends at sample PTS 48,318,384 and 6,331,968 respectively; container durations measured 1006.633333s and 131.916667s.

Prepared `metadata.json` describes these actual generated bytes. Native capture statistics, adapters and QPC evidence are removed; producer backend is `synthetic_fixture`. Original recorded time and media UUID are deliberately retained **only to associate the real telemetry with its substitute in this isolated fixture**. They are not evidence of original media content or identity. Calibrations and raw events were not changed.

SHA-256:

| Recording | Original metadata | Synthetic MP4 | Prepared metadata |
| --- | --- | --- | --- |
| Swiftplay | `cf26016648ae54fc49f0dc61dc6a7f822631190db08eac8902ecda3958a3b558` | `be85eda1cff0fd2ba2eb0e5a1021d4107ec852dda4d2552c7447e102639a5824` | `c8cc360be31f5a8edab2a8ce24583a63dce65f9f07a0fdc67aa503736ca9ac8f` |
| Practice | `1ee6423ce0706c716980170020015aa570a6ae424a4c644aebb3e2368c646a19` | `e3dc41cc326c4cd2559d293d222e3787ea48ecbb0cd1e7000c255517c918f4fe` | `762b7c7e48783e2f282b84391c12ffb63fef5f80866236f4df74bb328dd7022d` |
