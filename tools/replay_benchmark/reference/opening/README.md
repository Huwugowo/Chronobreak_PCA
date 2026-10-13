# Replay-opening v1 reference observer

Apply `reference.patch` only to the exact reference revision
`3a750e2c003d6e3605b2f907ff19ad7898d3dbd2` in an isolated `git archive` source
directory. The patch changes only `App.tsx`, `benchmark.ts` and
`components/ViewerScreen.tsx`; it preserves the eager library resources and full
playback-probe dependency. It adds the versioned H.264 workload admission and
bounded observation taps, preserving legacy and normal application behavior.

From the repository root, with a new empty destination:

```powershell
$referenceSource = 'build/qb010-opening-reference-new'
New-Item -ItemType Directory -Path $referenceSource
git archive --format=tar --output="$referenceSource/source.tar" 3a750e2c003d6e3605b2f907ff19ad7898d3dbd2
tar -xf "$referenceSource/source.tar" -C $referenceSource
git apply --check --directory=$referenceSource tools/replay_benchmark/reference/opening/reference.patch
git apply --directory=$referenceSource tools/replay_benchmark/reference/opening/reference.patch
Copy-Item tools/replay_benchmark/reference/opening/App.opening.test.tsx "$referenceSource/app/src/App.opening.test.tsx"
Copy-Item tools/replay_benchmark/reference/opening/ViewerScreen.opening.test.tsx "$referenceSource/app/src/components/ViewerScreen.opening.test.tsx"
npm ci --prefix "$referenceSource/app" --offline --ignore-scripts
npm run check --prefix "$referenceSource/app"
npm run test --prefix "$referenceSource/app"
```

The parity tests hold each real eager library dependency, HEVC and Data Dragon;
reject away/away-back Games paint; preserve all six warm cycles; and prove full
probe admission precedes media-open with the actual controller generation.
They are test-only files and are not part of the production observation patch.
The campaign must hash the exact patch and rebuilt binary before collecting any
run. These tests and the patch alone are not production performance evidence.
