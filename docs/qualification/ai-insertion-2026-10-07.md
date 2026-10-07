# Counted AI pauses, provider controls and native timing

Counted AI insertion, existing-Hold provider commands and native timing
disclosure are implemented, with focused native replay and real local-model
evidence on this Mac.
The repository gate is complete through the original run and targeted
continuation below. This record does not close DP-12; the remaining
implementation and qualification work is listed below.

## Behavior

`,a` requests a half-second pause; `3,a` requests 1.5 seconds.
`:ai-hold 750ms` accepts an explicit duration. Normal and Visual Edit use the
same cursor insertion rules as `,h`. The shared `insert_ai_pause` semantic
instruction resolves the duration in the captured project clock. One atomic
transaction commits the deterministic silent fallback and a durable AI
preparation for that new Hold. Original picture mappings and subsequent
speech retain their exact source coordinates.

Insertion requires no model. Its immutable birth controls are **Still**, no
plain-language guidance and no region target. Retry, reopening and Redo use
those saved choices, rather than whatever defaults the UI later supplies.
The bounded preparation worker performs conditioning off the project writer;
fulfilment records the request and first attempt only for a current claim.
Missing models or inputs leave the saved pause and an **AI PREPARATIONS**
entry in Jobs. **R** retries and **D** discards generation without removing
time. Undo removes the pause and cancels relevance; Redo starts a fresh
preparation. Ready pictures require explicit undoable acceptance.

Dot and macros retain the duration and AI intent. Each invocation resolves
its own target and allocates a fresh Hold and preparation. They do not reuse
a recorded Hold or generated artifact. Both `:hold` and `:ai-hold` capture
the insertion context and rate when command entry opens. A revision or
navigation mismatch refuses with an actionable message and makes no edit or
preparation; a late service reply cannot supply a new target.

`:hold-provider ai` requests pictures for the captured existing Hold and
adds no time. `:revert-ai`, also spelled `:hold-provider fallback`, restores
that Hold's captured deterministic fallback through the typed edit service.
The capture includes session, revision, target and Repeat Default/Play scope,
including the absence of an eligible target. An accepted provider can be
reverted after intervening gain edits by entering a fresh command; the revert
is one undoable revision and preserves those edits, timing, audio and framing.
A selected Play isolates only that occurrence. Explicit reversion also
supersedes pending preparation when the fallback already matches, without
creating a redundant Play override or clone.

Schema 71 and core document schema 46 deliberately refuse older unused
development packages under the session's breaking-format authorization.

## Exact timing shown in the inspector

The report distinguishes four intervals. For project rate `r`, current Hold
length `N_current`, original sampling length `N_sampled`, native frame count
`M` and native rate `q`:

| Quantity | Exact value |
| --- | --- |
| Current inserted pause | `N_current / r` |
| Requested boundary-to-boundary span | `(N_sampled + 1) / r` |
| Native boundary-to-boundary span | `(M - 1) / q` |
| Encoded native movie duration | `M / q` |
| Motion speed relative to native | `((M - 1) / q) / ((N_sampled + 1) / r)` |

The report uses the admitted plan, verified Ready receipt or accepted durable
sampling map, bound to the current session, revision and authoring target.
It performs no model work. Shortening an accepted Hold reports its current
prefix length while retaining the original sampling map and motion speed.
These are intrinsic Hold intervals, before an outer Repeat or Retime.

A speed at or below 0.95×, or at or above 1.05×, receives a visible heading
disclosure. This 5% threshold is a disclosure policy, not a quality guarantee;
smaller conversions remain available in the exact expanded report. Keyboard
focus scrolls the heading into view, and Enter expansion brings the interval
details into view. Conditioning endpoints are excluded from the inserted
interior pictures.

## Verification and retained failures

Evidence root: `/tmp/deadpan-resume-20261006`. Native replays used private
debug binary copies on Apple M5 Max, macOS 26.5.2, Rust 1.97.1
(`8bab26f4f 2026-07-14`), a 1280×820 viewport and a 60 Hz harness.
First import/index was cold; later operations were warm. OS cache, power and
thermal state were uncontrolled. These runs are functional and visual
evidence, not performance measurements.

| Check | Result and evidence |
| --- | --- |
| First focused native batch | Three scenarios passed: `ai-pause` (17 checks), `ai-scoped` (41), `ai-replacements` (15). `ai-variants` and `ai-insertion` failed harness assertions described below. `ai-insertion-replays-1/summary.json` and `ai-insertion-replays-1.log` retain the failed invocation. |
| Corrected native scenarios | All 88 checks passed: `ai-variants` 43 and `ai-insertion` 45. `ai-insertion-replays-2/summary.json`, both scenario `report.json` files and `ai-insertion-replays-2.log`. This rerun covered the two corrected scenarios, not the complete replay catalog. |
| Workspace gate run | 4,981 tests ran: 4,979 passed, two failed and ten skipped in `ai-insertion-gate-2.log`. The two failures were outdated expectations for the counted `,a` hint and database schema 71. Both corrected tests passed in `ai-insertion-gate-final-fixed-tests.log`. |
| Gate continuation | Formatting, strict workspace Clippy and strict UI Clippy passed. All 1,056 UI-harness tests passed, with two skipped, in `ai-insertion-gate-final-ui-tests.log`. Both workspace doc tests passed, with none ignored, in `ai-insertion-gate-final-doctests.log`. `ai-insertion-gate-final.json` and `ai-insertion-gate-final.log` preserve every continuation stage and its exit status. |

The gate was completed in parts; the failed `ai-insertion-gate-2` invocation
remains a failed invocation. Passing workspace tests were not rerun solely
to replace that record. Nextest reported `LEAK` for two passing tests:

- Workspace, `deadpan-track`:
  `faces::tests::vision_boxes_become_ordered_clipped_displayed_rectangles`.
- UI, `deadpan-app`:
  `gag_presets::tests::one_unreadable_entry_is_skipped_and_kept_while_the_rest_load_and_save`.

These warnings remain in the logs. Nextest also marked 14 workspace tests
and eight UI tests slow.

The insertion replay checks 45 inserted frames at cursor 10 on the qualified
30000/1001 project clock, raising the fixture from 120 to 165 frames. It checks
all retained Original mappings exactly and resolves every frozen picture
against the qualified index. It also exercises automatic unavailable-model
preparation, Jobs Retry/Discard, Undo/Redo, dot, macros, explicit duration and
command-entry rejection after delayed service replies. Provider replays cover
generation without added time, explicit acceptance, scoped reversion, later
gain preservation and exact-revision refusal.

These scenarios use scripted model unavailability and a synthetic qualified
Ready worker. They exercise native routing, the production service and store,
media qualification and Metal presentation, but do not demonstrate real
inference, physical audio output or perceptual quality.

Review and failed runs produced these corrections:

- Command entry initially recaptured the insertion target at Enter. The
  shared pause dispatcher now consumes its original capture and rejects
  changed context. Native replay checks both `:hold` and `:ai-hold`.
- Explicit scoped fallback reversion now retains provider-change intent even
  when the fallback already matches, cancelling pending work without an
  unnecessary occurrence override.
- The timing heading and expanded body were clipped despite accessible text
  assertions. Focus/expansion now scroll them into view. Replay asserts full
  paint visibility, an unobscured hit rectangle and the actual interval lines.
- The first stale-provider replay tried to open command entry while a macro
  acknowledgement was pending, so no revert was submitted. It now opens the
  command first, delivers an independent typed gain edit, then submits the
  captured command. It waits for the service's `Project changed` error and
  checks the unchanged head, document and generated provider.
- The first insertion replay equated a Source sample's frame-center
  coordinate with a Freeze's measured picture PTS. It now checks the exact
  indexed ordinal, PTS and timebase for the frozen sample; all retained
  Original coordinates still require exact equality.
- The first focused compile caught a missing `eframe::egui` import. The
  first gate caught the large preparation-origin enum; its retained accepted
  artifact is now boxed. Failed logs remain in
  `ai-insertion-focused-1.log` and `ai-insertion-gate-1.log`.

Offline inspection of actual captured PNGs confirms the expanded timing
heading and interval text in
`ai-insertion-replays-2/ai-variants/ai-variants-036.png`. It shows the exact
1250/1001× speed and the distinct inserted, requested, native-boundary and
movie durations. `ai-insertion-replays-2/ai-insertion/ai-insertion-037.png`
shows the 45-frame silent Freeze, retained picture 009, 165-frame Edit and
discoverable provider/revert controls. Additional retained captures cover
stale refusal (`ai-variants-082.png`), reversion after later edits
(`ai-variants-090.png`), dot/macros (`ai-insertion-080.png`) and captured
pause commands (`ai-insertion-101.png`) in their respective scenario folders.
Layout retries settled; the reports retain retry warnings rather than
treating them as absent.

## Reproducibility

Base revision was `2bf4cd2f84f3cdc96471936f3ce77df573a9fbf8` with uncommitted
changes. Each replay report records checkout metadata at replay start;
the binary hashes identify the executed builds. Both summary files retain
all helper paths and hashes. These are SHA-256 values:

| Artifact | Hash |
| --- | --- |
| First batch `bin/deadpan-app` | `f0d9179511b1afbaada179ff54e23d27f90c8f930f2382fe91d80ce636584e4a` |
| Second batch `bin/deadpan-app` | `47993bffeea5303c787b11b28abd7e4688f51f6ec9f2fa6ae09444d3ac74fa90` |
| Second batch `bin/deadpan-media-worker` | `46e6ea648871a3285a5f0be5804c80d1edd3eec22b13d79d5d911f218b9c0e7b` |
| Second batch `bin/deadpan-track` | `8df4567edcdb362bce22903d14accd2f3947a3231d25a524d2e23e3fca902a7e` |
| Second batch `bin/deadpan-transcribe` | `9c4da2c84646f5d143b73e5182c93d9998252fb3eee4ae5b0a34aa6a0b3ebacc` |
| `native/deadpan-source/tests/fixtures/cfr-bframes.mp4` | `5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918` |
| `Cargo.lock` at second replay | `5e3cc218390e47cea32c662e42951be5b5bdd8459a12df4658c34115881d3780` |
| Tracked diff at second replay | `22762a812d3461f22b0801b02cbb2ec4d533679843f11af130f3a2dd4845d303` |

The evidence root also contains per-file source manifests produced by
`insertion-source-proof.py`. Their names identify successive captures, not
equivalent source trees. Each records the base above, timestamp and file
hashes, including new source files. The recorded tracked-diff hashes are:

| Manifest | Files | Tracked-diff SHA-256 |
| --- | --- | --- |
| `insertion-source-before-gate.json` | 55 | `d42ddfeadab92962539d46e2f8893a27d9db4f4ddb03b7804e0b8bc5aa4a11db` |
| `insertion-source-final.json` | 55 | `af5e9f14328f9a87303dbd55552044e4aa52e2bccb6286354db629a14e056f29` |
| `insertion-source-boxed.json` | 55 | `ebe7b57d1324c190d29fbe136c24dec5e8a9c2b4d29ef453a251f9e524913c6d` |
| `insertion-source-test-fixes.json` | 56 | `5af49329995d5d8357fad96b32a670f8c82625974327d9a0a4532638a09512f3` |

After the gate continuation, the latest manifest
`insertion-source-test-fixes.json` matched all 56 recorded source paths.

## Real local-model insertion

`ai-insertion-generation-2/summary.json` records **79 passing checks** from
`verify-ai-insertion.py`. Private app and CLI binaries in
`insertion-model-build/bin` exercised the production headless path with the
installed `ltx-2.3-q4-bridge` pack and runtime `0.15.8+deadpan5`. The run
preserved HOME and used explicit development runtime paths. Its request,
worker binding, host provenance and retained sampling plan agreed.

A semantic insertion at frame 20 committed a 30-frame silent Freeze and one
queued preparation before model access. The complete inserted audio-plan
interval was silent Hold content; sampled start, middle and end PCM blocks
were exact digital silence. The actual model reached Ready without changing
the saved document. Explicit acceptance installed generated pictures and
retained the exact fallback. After a separate gain edit, explicit revert
changed only the provider, and Undo restored acceptance while keeping gain.
Shortening to 24 frames reused the original artifact and sampling map,
created no preparation and retained the checked sampled-master ordinals.

The run compared 256 exact stereo PCM frames around the fixture's impulse
at Original sample 48,000. The fixed baseline window was `[47904, 48160)`,
with peak 0.749076783657074. Insertion was at `B(20) = 32032`, so the probe
offset was 15,872 samples, beyond the 96-sample seam fade. The same offset
produced `[95952, 96208)` after insertion and reversion, and `[86342, 86598)`
after shortening. Each block equalled the baseline sample-for-sample,
without alignment, event searching, dropped samples or a tolerance.
The resumed Original picture also retained its exact source coordinate.

`ai-insertion-generation-1/summary.json` retains the failed first attempt.
It stopped before insertion because its probe `[96352, 96608)` fell in
fixture silence. The fixture generator authors impulses at sample 100,
48,000 and 200 samples before its end. The correction moved insertion from
frame 60 to frame 20 and chose the fixed impulse window above, with an
explicit nonzero-amplitude check. This was a qualification-script correction;
production code and the exact preservation assertion were unchanged.

With model-cache and Python paths explicitly set to absent locations, the
accepted shortened project reopened, validated and exported through the
production emitted-file verifier. `ai-insertion-generation-2/Exports/ai-insertion.mp4`
was published at 115,942 bytes and contains generated pictures; export left
authoring unchanged. A separate macro project ran the same saved AI insertion
twice, creating two distinct Holds and preparation identities. One Undo
removed both pauses and cancelled both pending intents.

The generation drain took 96.075 seconds, including a recorded 92.394-second
worker stage. Verified Render took 36.016 seconds and the whole script
135.763 seconds. These observations occurred alongside other qualification
work and are not quiet performance measurements. This fixture establishes
exact media behavior, not perceptual quality or spoken-dialogue judgement.

The actual intrinsic intervals, retained in the summary, were:

| Quantity | Exact value |
| --- | --- |
| Inserted 30 project frames at 30000/1001 fps | `1001/1000 s` |
| Requested 31 boundary intervals | `31031/30000 s` |
| Native 24 boundary intervals at 24 fps | `1 s` |
| Native 25-frame movie at 24 fps | `25/24 s` |
| Retained motion speed | `30000/31031 ×` |
| Shortened 24-frame pause | `1001/1250 s` |

The generation summary retains hashes for all executed helpers, the worker
script, native and sampled objects, provenance and model manifest. Primary
SHA-256 identities for this run are:

| Artifact | Hash |
| --- | --- |
| `insertion-model-build/bin/deadpan-app` | `fd1857947ec7d792c0bfef2af9e58743e2462a77e7c6c546e66a179890e0dc53` |
| `insertion-model-build/bin/deadpan-cli` | `0ce9cd120ff4ab80cc410173bb33a010b23d370dc4ef967c9cec536ea8e7d33c` |
| `verify-ai-insertion.py` | `4f545d5b7edfe6f5d5234cbf4041b4f0fa8ebd6e7525c1a5f3415e155f16092e` |
| `cfr-bframes.mp4` fixture | `5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918` |
| Published `ai-insertion.mp4` | `8d2b98de05cfbb5d32abfc00ce186e03bf808fd23cc436217b9b4e4e5098728d` |
| Model manifest | `9ba32055ae5df43ca6ecbe0c492de4d6d7a41c9e26a6d0f0b7bdb091818fe466` |

The first release bundle and its separate packaged checks are recorded below.
The development-runtime export above is distinct evidence from those paths.

## Exact command grammar follow-up

Specification §6.4 also requires `:hold 1.5s video=ai audio=silence`.
The first implementation still rejected that spelling. It now resolves to
the same captured `InsertAiHold` action as `:ai-hold`, including exact units,
zero-duration no-op, independent parameter order and duplicate/audio errors.
Independent review found no remaining issue in this follow-up.

All 47 parser and registry tests passed in each of the base and UI-harness
configurations, completed in parts. The first run passed 46 and failed the
routing snapshot: changing a command usage needs a second snapshot generation
because its example inputs come from the previous embedded baseline.
`ai-insertion-grammar-routing-test.log` and
`ai-insertion-grammar-routing-ui-test.log` record the corrected single-test
reruns. Both strict Clippy configurations and formatting passed. The two
reference-generation invocations retain their intentional rewrite panics.

`ai-insertion-grammar-replay/summary.json` passes all 57 checks with private
app SHA-256
`872866511792cb6f1ed0c59edb8c8a59e9d27fed23eae79233ce4c6bb715dbfe`.
It submits the exact spec command through native text entry, checks the fresh
silent Freeze and same-revision durable intent, removes both with one Undo,
and refuses the command after a delayed history reply changes its capture.
Actual captures `ai-insertion-020.png` and `ai-insertion-121.png` were inspected:
the retained picture, separate clocks, selected pause, provider controls,
keyboard hints and final refusal are visible. The run retains settled-layout
warnings and is not a quiet performance measurement.

`insertion-source-grammar.json` records all 56 affected source hashes after
this follow-up. Earlier binary and source identities above remain distinct.

## Release checks and additional comparison

`bundle-insertion-verify.log` passes the relocated release bundle's positive
checks and damaged-helper/runtime refusals. The bundle also reopened and
rendered the accepted, shortened project in a new empty home. Doctor confirmed
that no model was ready; the authored document stayed unchanged.
`ai-insertion-packaged-export-1/summary.json` records:

| Object | SHA-256 |
| --- | --- |
| Release app | `9a01cf20e32a9aeab7ac0dbafe5867c45a7239a723bf00a84ad720f4e38f6261` |
| Release CLI | `bcac3e27d4fa37780063421714cf7223ea2c4aeabf582a6f14ba6de708391d30` |
| Verified MP4 | `62bf0e67804d6d47068f901ce076f901c6fb79d1dab7f90edac5f7e84cdb0ebd` |

This bundle precedes the exact-command follow-up. Its finished-file verifier
passed; the separate complete preview-versus-export diagnostic flagged three
generated pictures in `ai-insertion-preview-export-1.json`. All 144 picture
clocks and five audio windows passed, with zero measured offset in all three
signal windows. Picture 33, 35 and 39 each preferred the preceding reference
under the neighbor heuristic despite 55–57 dB own-picture PSNR. Investigation
and a known-order low-contrast compression witness are retained in
`export-insertion-diagnosis`. That schema-2 invocation remains failed;
the finished-file result does not replace it. The correction and separate
schema-3 results follow.

### Correcting the neighbor-index claim

Independent decode and reference extraction reproduced all three comparisons.
The adjacent references differ by only 0.4180, 0.1986 and 0.2560 luma RMS codes.
A separate correctly ordered three-frame encode (flat 100, a 99/101
checkerboard, then flat 160) loses the middle frame's one-code texture under
libx264 CRF 23. Its own 48.13 dB PSNR passes the existing fidelity gate while
the previous reference matches exactly. This proves a false positive in the
old neighbor heuristic; it does not prove the encoded source identity of the
three product frames. Production ordinal-handoff inspection found no defect.

Report schema 3 now exposes each adjacent reference pair's RMS separation and
whether it is distinguishable at the existing declared luma fidelity. The
reference-only threshold is twice `peak * 10^(-minimum_psnr / 20)`. At or below
that separation the allowed error regions overlap. The raw neighbor PSNR stays
visible, but cannot justify an index-mismatch claim. The rule applies equally
to Original, Generated and Background pictures. It never derives a larger
allowance from the decoded residual. Pixel, structure, black, color, PTS, count
and audio gates are unchanged. Unobservable comparisons are counted explicitly
and do not establish exact source identity. See
[the verifier contract](../PREVIEW_EXPORT_VERIFICATION.md#picture-gates).

Independent review found no remaining issue. All 13 focused verifier tests pass
in `ai-insertion-index-unit-tests.log`, including bounded texture loss and
distinguishable wrong-frame controls in SDR/HDR, exact threshold equality,
separate branch thresholds, missing/static neighbors and report coverage.
Formatting and both strict Clippy configurations pass. The optimized
`preview_export` and `preview_export_hdr` integration targets pass all nine
tests with none ignored, covering real SDR recipe exports, PQ/HLG public and
worker exports, negative picture/audio/timing controls and output selection.
`ai-insertion-index-checks.json` retains commands, exits and elapsed times;
`ai-insertion-index-real-media-tests.log` retains the test results.

The final release bundle in `bundle-insertion-final/` passes relocation,
signature, helper/runtime integrity and damaged-baseline refusal checks. It
reopens and renders the accepted project in a fresh HOME with no model ready,
without changing authoring. `ai-insertion-final-release.json` records each
separate stage and `ai-insertion-packaged-export-final/summary.json` records:

| Object | SHA-256 |
| --- | --- |
| Final release app | `5ff45b69f1163fb2057604546417088974f73cff1c5206897be9d032ab179adb` |
| Final release CLI | `30cf5176c15a5733833736d0a8143655eed4662a6006c063c072ba83c684490f` |
| Final verified MP4 | `27ac097f5f0f35186bc3bdcbaa0ca0fa6c096053a84698f18ed52bdedae35298` |

Both `ai-insertion-preview-export-retained-v3.json` and
`ai-insertion-preview-export-final-v3.json` pass all 144 picture checks and
five audio windows. Three signal windows report zero offset. Minimum luma
PSNR is 54.742 dB, minimum chroma PSNR 57.561 dB and maximum local error
2.125 codes. The 286 directed neighbor comparisons contain 176 observable
and 110 unobservable pairs. The three formerly flagged pictures are explicitly
unobservable; their raw negative margins remain in the reports. No exact
source-identity claim is made for those pairs. The original schema-2 failure
remains retained and is not described as a passing invocation.

## Complete native replay run

`ai-insertion-replays-all/summary.json` records 68 passing scenarios, one
failure and two fixture-only skips, across 7,168 checks. Its frozen app
SHA-256 is `dbb9f86e24fb1e63c415dfb4904841c61ad5326426c37dd81951cb27bdafe4a1`.
The run precedes the exact-command and verifier follow-ups; the 57-check
command replay above covers the grammar change. All prior successful results
are retained. The run took 2,664.5 seconds wall time with three concurrent
scenarios, alongside compilation and some earlier model work; it is not a
performance measurement.

Trim stopped after 93 checks with `No real paste update to hold`. The harness
waited for the service to become idle, then required one immediate mailbox
read. The service releases its busy guard before publishing, and the mailbox
read can also miss a held lock. The correction waits for and retains the
actual new commit for the captured session/project/revision while UI delivery
stays withheld. It preserves the real paste and every assertion and adds
mailbox diagnostics on timeout. Root review found no product change in this
correction. `ai-insertion-trim-final/report.json` passes all 166 Trim checks
plus the keyboard audit. The final build also passes the complete 133-check
editing-to-export replay in `ai-insertion-full-session-final/report.json`,
including the corrected verifier. Formatting and strict UI Clippy pass again
after the harness-only correction (`ai-insertion-final-ui.json`).

All 69 runnable scenarios have therefore passed, completed in parts. The
original full-run invocation remains failed. `ai-pause-ready` and
`generated-picture` retain their explicit fixture-only skips. The separate
real-model run above establishes actual generation/acceptance/export evidence.
Final offline capture inspection covered the reopened session
(`full-session-127.png`) and Trim's retained exact proposal with zero audio
context (`trim-123.png`); controls, pictures, focus and feedback remain visible.
No native app window was opened for these checks.

## Separate release responsiveness sample

`ai-insertion-performance-final/` contains five sequential performance-mode
runs after compilation, broad visual replays and inference finished. All 1,408
checks pass. The frozen release harness SHA-256 is
`2c337c7fbeb74e27e62635ac0131253d92f86061aabc52b459e974261eac4776`.
`runs.json` retains per-scenario wall time and pre-run load (one-minute values
2.9–3.9). The per-scenario reports retain all samples and thresholds; no PNG
readback is included. Hardware, raster, cache and uncontrolled power/thermal
conditions match the metadata described above.

| Scenario | UI CPU p95 / max (ms) | Input-to-commit p95 (ms) | Commit samples |
| --- | --- | --- | --- |
| AI insertion | 2.001 / 6.380 | 89.823 | 9 |
| AI variants/provider controls | 0.294 / 5.968 | 90.963 | 5 |
| Rapid input | 0.468 / 94.341 | 123.279 | 15 |
| Edit latency | 0.786 / 5.401 | 12.018 | 89 |
| Large project | 0.540 / 5.423 | 85.401 | 1 |

The aggregate rows include initialization and deliberate delayed-reply cases;
small sample counts do not establish population percentiles. In the explicit
120-sample warm-navigation workload, input CPU p95 was 0.244 ms and input to
offscreen picture completion p95 was 1.338 ms (maximum 1.439 ms). The full
rapid-input run retains its 94.341 ms UI outlier and 219.508 ms maximum
input-to-picture delay. These results do not establish physical display or
all Section 25 budgets, and do not close DP-18/DP-24.

`insertion-source-index.json` retains the 60 source hashes after the verifier
correction. `insertion-source-trim.json` retains the final 61 paths, including
the harness correction, and a final comparison found no source drift.

## Remaining DP-12 work

Changing source boundaries still needs the automatic fresh-request lifecycle
required by §12.7, while retaining an explicit fallback. Explicit generation
with changed motion controls already starts a new request. The one-sided
extension path still needs qualified
lead-in conditioning and an explicitly unconditioned outgoing seam where no
right-hand bridge is available (§12.2). Counted insertion does not make an
unsupported bridge or one-sided request supported.

Physical/perceptual checks outside this Mac's automated scope remain subject
to the owner's §29.1 verification list.
