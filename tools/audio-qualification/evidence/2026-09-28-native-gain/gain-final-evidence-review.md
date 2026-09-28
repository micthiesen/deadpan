# Native gain evidence inventory review

Historical review of focused replay 09 and the evidence available at that point.
Its pending rows and proposed inventories are not final qualification claims.
The retained `summary.json` records terminal command/replay outcomes, and
`docs/qualification/native-gain-2026-09-28.md` records the final source and scope.

Read-only inspection of existing reports and captures, 2026-09-28. No builds, tests, native UI or GPU runs by this reviewer. Source/docs changes are separately assigned. Parent owns final qualification.

## Passing focused replay 09

- Invocation: `target/debug/deadpan-app --ui-check --scenario gain --retain-projects --output /tmp/deadpan-native-gain-u891jpt2/gain-visual-09 --kestrel-source /Users/michael/.dotfiles/kestrel/Sources/Kestrel/Shortcuts.swift`.
- Exit 0; external elapsed 121.947521416 s, replay-only elapsed 116.512426708 s. These are different scopes, not product performance measurements.
- Source manifest SHA-256 `d70a83a97df033c1bde0be8f159f4b37348e4b02a8dd63c05f0e5217a16ff69a`; executed debug binary SHA-256 `a338a7a0eee05992b1230fd1ec1084d5b66308bfc755987fe9e56a5481229b13`.
- Base commit `565ea9d07d8044816f423fec6b299763f84d7d47`; report tracked-diff SHA-256 `20a547446573adc59e578271156ac6bcee060377bfd651f2980c05609c0139d2`. Use source manifest and binary identities for tested source, since the run wrapper's diff includes a different checkout snapshot.
- Apple M5 Max, macOS 26.5.2, Rust 1.97.1; Metal renderer. Fixture `cfr-bframes.mp4`, SHA-256 `5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`.
- **266 gain checks plus 1 Kestrel check = 267 total; zero failures.** Gain records 2,746 semantic frames and 123 screenshots. Kestrel check covers 5,456 routing cases, 62 reserved bindings, no conflict or live-source drift.
- One warning: intermediate screenshot allowance reached; semantic frames continued and reserved named screenshots were captured. Do not describe this as a missing semantic test.

Meaningful assertions cover counted +/- and direct trim/mute undo; Original/Sources target refusal; proposal identity/no-history; buffered fields, superseded success and coalescing; exact envelope [1/2,179/2), interior key91/2 at+6dB, final-9.5dB Smoothstep, and mute[3/2,5/2); unchanged active Before/Draft tickets; exact137-sample heard position after loop wrap; stale updates; invalid field Pause/Restart behavior and reset; fault pause/no automatic resume; one Apply/undo; picture-preserving cancellation; native text/IME ownership and boundary wrapping.

Populated keyboard checks are substantive: four complete circuits (Tab and Shift+Tab at each viewport) visit34 controls each, giving136 enabled/full paint/full hit-clip checks without reveal/wheel input. The proposal/history stays fixed. At960x640 the painted viewer is145pt (minimum140), graph106pt inside109pt scroller. At1280x820 viewer270.1875pt (minimum230), graph106pt inside163.8125pt scroller.

Explicit limits: audio delivery in replay is injected typed updates, not PCM preparation or an open audio device. Real qualified PCM/cache separation belongs to playback tests. OS picker selection is scripted. Physical display presentation, VoiceOver, OS IME delivery and acoustic quality are unqualified. Broad visual suite remains separately pending/failed until its terminal evidence is recorded.

## Selected final captures

Both images inspected directly. They show retained picture, visibly unsaved owner, authored curve with quarter-duration ticks, and fixed comparison/commit actions; no waveform is claimed.

| File | Frame | Caption | SHA-256 |
| --- | ---: | --- | --- |
| gain-visual-09/gain-121.png |626|960x640: complete authored curve and fixed controls retain145pt picture; exact fields are below the real scroller.|bb2fcafd7dc5a57929660c80b7af783c7037ece7043015b6ba6447dd4a4cf419|
| gain-visual-09/gain-122.png |1660|1280x820: complete curve beside exact fractional range/key fields, unsaved inspector and fixed Before/Draft actions.|6aafb0c855094e34f971be07ac47ad5b61aac69d3ec74becad599605333d558d|

Optional third named checkpoint: gain-123.png (frame2549), “Unsaved gain draft ready for Before and Draft comparison”, SHA-256947274c349cdacc1527cd14a05070e19a458b2060cbc8babef51d586b00f28a4. It is not needed for the preferred two-image comparison and has not been independently visually reviewed here.

## Meaningful retained failures

| Run | Outcome | Minimum useful evidence |
| --- | --- | --- |
|01|Harness lookup missed ComboBox value Linear.|run JSON/log plus bounded failure summary; retain raw report compressed if retaining all diagnostics.|
|02|Harness queried provisional disabled popup geometry before settled enabled item.|run JSON/log plus bounded failure summary.|
|03|Real minimum viewer127pt failed140pt minimum.|report check plus gain-121.png failure image.|
|04|Real curve menu extended beyond960pt viewport to986.03125pt.|finding plus gain-121.png failure image.|
|05|Real Tab traversal lost focus after Cancel before heading.|failed focus sequence and gain-124.png; image alone does not establish keyboard behavior.|
|06|Harness owner-axis selector also matched In/Time labels; graph itself was fully visible.|failed check and bounded selector correction note.|
|07|Passing pre-populated-keyboard baseline:126 gain checks.|run summary; don't count as final coverage.|
|08|Real first Tab at960x640 focused trimmed-off Whole beat trim field, paint[] despite focused enabled TextInput.|failed check plus gain-122.png failure image.|
|09|Passing final focused266 gain checks.|complete report and two selected images above.|

Keep broad full-ui-visual failure record too: parent reports one workspace picture-boundary floating-point delta0.0000153pt under investigation. Do not label the broad suite passing based on focused gain09.

## Proposed final compact evidence manifest

Use the existing qualification directory convention. Prefer original JSON/log bytes compressed losslessly where large, with a manifest of source-relative filenames, byte sizes, SHA-256, run/source identities and pass/fail counts. Keep:

1. Final source inventory d70a...; command/exit/timing JSON and logs for base workspace tests, final feature app294 tests, final strict lint/format, focused09 and terminal broad visual run/any justified continuation. Preserve failed original command records; don't replace them with rerun results.
2. Focused09 full report.json (26,110,248 bytes; SHA-2565f7271376902a5f8b70f44f4e2067d8885cfaceb3c2bc9c5b3e4634a84c4cdb7), plus selected121/122 PNGs. Report HTML duplicates report payload (~23.9MB) and expects image links. Omit HTML in a compact selection, or preserve all123 PNGs (~36.4MB) if preserving navigable HTML; explicitly document any unretained image references.
3. Original failure reports01-08 compressed plus wrapperJSON/logs and selected failure screenshots03/04/05/08. Add a compact derived failures.json or Markdown index identifying harness defects versus product defects. Preserve raw failed reports so conclusions remain reviewable; bounded extracts can be displayed in docs.
4. Terminal full visual report and any final app/lint failures/recovery logs. Final results are parent's responsibility; this inventory does not infer their terminal status.
5. Small top-level README/manifest with authority/limits, exact267 focused total, separate294 app tests, separate2029 base tests, and links to native-gain qualification. Never add those overlapping populations into a misleading unique-test count.

Exclude retained `projects/**`, SQLite packages, media snapshots/original fixture copies, executables, target artifacts, caches, temporary build products and private native QA outputs. `--retain-projects` roots remain local for parent manual QA and are not evidence publication assets. The report records local Documents paths as metadata, not package contents.

## Parent retention scope clarification

Final evidence destination: `tools/audio-qualification/evidence/2026-09-28-native-gain/`. Preserve every full `source-*.json` inventory referenced by a verification run, every named command JSON, and gzip-compressed complete logs, including failed commands. Do not retain duplicate script plans as evidence copies. Current inventory follows; pending rows are not completion claims. Parent will add terminal native/release/base records. The broad visual original stays failed even if the affected workspace follow-up passes. Its planned picture-mesh precision correction uses 0.01 physical-pixel containment tolerance with exact regressions; text/control clipping is unchanged.

| Command record | Recorded exit | Seconds | Source manifest |
| --- | --- | ---: | --- |
|axes-clippy-ui.json|0|28.494|cac790383d78168764e979756acc2b66269c2b7663166e7b9665b61ab5c43930|
|axes-format-check.json|0|4.269|cac790383d78168764e979756acc2b66269c2b7663166e7b9665b61ab5c43930|
|axes-format.json|0|4.242|d8b085ffbb1509f98bcbc2c6e6af804b18511d85ef8b13cfadc441b014a88515|
|axes-ui-artifacts.json|0|30.576|cac790383d78168764e979756acc2b66269c2b7663166e7b9665b61ab5c43930|
|clippy-base-final.json|0|34.632|f8e4b0093aad22254690fe1025da1d91459bed347d9c50d82de3caa97ba79ff2|
|clippy-base-initial.json|101|58.291|ddf80d6f1b58e319d1263aa1a1a5e296d7a27e53fd33ac7829474ab1fe6ff5ab|
|clippy-ui-final.json|0|41.458|f8e4b0093aad22254690fe1025da1d91459bed347d9c50d82de3caa97ba79ff2|
|clippy-ui-fixed.json|0|274.591|1012fcbbbebe17537f27f1e388c1c93d07a94ecae98eadc0c2679a0b944e720a|
|clippy-ui-initial.json|101|692.707|c69a5ea5054012e4c7d58fd2749a2aeb926c1067a77c34b84dc200e22a44f0d5|
|final-ui-app-tests.json|0|7.654|d70a83a97df033c1bde0be8f159f4b37348e4b02a8dd63c05f0e5217a16ff69a|
|focus-clippy-ui.json|0|29.155|b3a4efb96350e0a5aa6eeb3c889e57f0ff561d2f68d9144d416865a24f811302|
|focus-format-check.json|0|4.298|b3a4efb96350e0a5aa6eeb3c889e57f0ff561d2f68d9144d416865a24f811302|
|focus-format.json|0|4.17|f93f6bd16346e1a4da7b76391cc34528c84241433de7e3e0e4cbd1fa9248cf12|
|focus-ui-artifacts.json|0|40.101|b3a4efb96350e0a5aa6eeb3c889e57f0ff561d2f68d9144d416865a24f811302|
|format-apply.json|0|4.308|44535878b9b69fec3655f85578938920d2ccde3cca07836d1e7ad4ac23e8f407|
|format-final.json|0|4.4|f8e4b0093aad22254690fe1025da1d91459bed347d9c50d82de3caa97ba79ff2|
|full-ui-visual.json|pending|0|d70a83a97df033c1bde0be8f159f4b37348e4b02a8dd63c05f0e5217a16ff69a|
|gain-visual-01.json|1|59.395|f8e4b0093aad22254690fe1025da1d91459bed347d9c50d82de3caa97ba79ff2|
|gain-visual-02.json|1|53.816|6ae61bbedde1c918c8ce3e99cbd151ca44145fdd02306ca9a407ccc2c936721d|
|gain-visual-03.json|1|49.782|a4ba31db8f9ef884be7718b9b924a9179f408f3b761824ddf393da3d5adaca40|
|gain-visual-04.json|1|61.365|1e522187374c4cebb803c610e8d9e412985237991bac49dc66d96c59ed900962|
|gain-visual-05.json|1|69.451|eb466626c7a76801dc170c179e3c17437d7f941429790d181c710b5735c0100e|
|gain-visual-06.json|1|72.573|b3a4efb96350e0a5aa6eeb3c889e57f0ff561d2f68d9144d416865a24f811302|
|gain-visual-07.json|0|77.889|cac790383d78168764e979756acc2b66269c2b7663166e7b9665b61ab5c43930|
|gain-visual-08.json|1|68.956|6e6d31d52bd4742dbc0413b54782fd29fc99ee055a487d5c55e2249437e03660|
|gain-visual-09.json|0|121.948|d70a83a97df033c1bde0be8f159f4b37348e4b02a8dd63c05f0e5217a16ff69a|
|layout-clippy-base.json|0|27.425|6ae61bbedde1c918c8ce3e99cbd151ca44145fdd02306ca9a407ccc2c936721d|
|layout-clippy-ui.json|0|28.768|6ae61bbedde1c918c8ce3e99cbd151ca44145fdd02306ca9a407ccc2c936721d|
|layout-format-check.json|0|4.245|6ae61bbedde1c918c8ce3e99cbd151ca44145fdd02306ca9a407ccc2c936721d|
|layout-format.json|0|4.01|51054ddb1c8b5adff829f4b4501fbd2459617d3bf125c3b655e1a810b83f6f27|
|layout-ui-app-tests.json|0|8.04|6ae61bbedde1c918c8ce3e99cbd151ca44145fdd02306ca9a407ccc2c936721d|
|layout-ui-artifacts.json|0|35.651|6ae61bbedde1c918c8ce3e99cbd151ca44145fdd02306ca9a407ccc2c936721d|
|minimum-clippy-ui.json|0|28.216|1e522187374c4cebb803c610e8d9e412985237991bac49dc66d96c59ed900962|
|minimum-format-check.json|0|4.337|1e522187374c4cebb803c610e8d9e412985237991bac49dc66d96c59ed900962|
|minimum-format.json|0|4.211|1e522187374c4cebb803c610e8d9e412985237991bac49dc66d96c59ed900962|
|minimum-ui-artifacts.json|0|29.415|1e522187374c4cebb803c610e8d9e412985237991bac49dc66d96c59ed900962|
|populated-clippy-ui.json|0|28.412|6e6d31d52bd4742dbc0413b54782fd29fc99ee055a487d5c55e2249437e03660|
|populated-format-check.json|0|4.289|6e6d31d52bd4742dbc0413b54782fd29fc99ee055a487d5c55e2249437e03660|
|populated-format.json|0|4.238|53d283d301408e993241eb8034ccf1b300471b46068f4e72d08e2c79b873eb8c|
|populated-ui-artifacts.json|0|34.843|6e6d31d52bd4742dbc0413b54782fd29fc99ee055a487d5c55e2249437e03660|
|popup-clippy-ui.json|0|28.691|a4ba31db8f9ef884be7718b9b924a9179f408f3b761824ddf393da3d5adaca40|
|popup-format-check.json|0|4.273|a4ba31db8f9ef884be7718b9b924a9179f408f3b761824ddf393da3d5adaca40|
|popup-format.json|0|4.192|7fe774332fe2e6de020007d1dcf451b9fef5ddb2931b16922baacc535f7cf63b|
|popup-ui-app-tests.json|0|7.186|a4ba31db8f9ef884be7718b9b924a9179f408f3b761824ddf393da3d5adaca40|
|popup-ui-artifacts.json|0|32.365|a4ba31db8f9ef884be7718b9b924a9179f408f3b761824ddf393da3d5adaca40|
|responsive-clippy-ui.json|0|28.283|eb466626c7a76801dc170c179e3c17437d7f941429790d181c710b5735c0100e|
|responsive-format-check.json|0|4.253|eb466626c7a76801dc170c179e3c17437d7f941429790d181c710b5735c0100e|
|responsive-format.json|0|4.241|eb466626c7a76801dc170c179e3c17437d7f941429790d181c710b5735c0100e|
|responsive-ui-artifacts.json|0|29.551|eb466626c7a76801dc170c179e3c17437d7f941429790d181c710b5735c0100e|
|reveal-clippy-ui.json|0|30.332|d70a83a97df033c1bde0be8f159f4b37348e4b02a8dd63c05f0e5217a16ff69a|
|reveal-format-check.json|0|3.395|d70a83a97df033c1bde0be8f159f4b37348e4b02a8dd63c05f0e5217a16ff69a|
|reveal-format.json|0|3.593|6a5713dc960564ea71c4e04b9923e27cdf1227168f291f7ce93fd3cbfa25a048|
|reveal-ui-artifacts.json|0|33.642|d70a83a97df033c1bde0be8f159f4b37348e4b02a8dd63c05f0e5217a16ff69a|
|test-base.json|0|1335.534|1e4d91ea1a8f17b8e8aef47156f4911db812fdfeb0ff681cf9c28f7590ae95b4|
|test-ui-app.json|0|8.761|f8e4b0093aad22254690fe1025da1d91459bed347d9c50d82de3caa97ba79ff2|
|ui-artifacts.json|0|1102.498|f8e4b0093aad22254690fe1025da1d91459bed347d9c50d82de3caa97ba79ff2|

This snapshot contains 56 command records referencing 20 distinct complete source inventories. Preserve other existing source inventories too if a run log references them.
