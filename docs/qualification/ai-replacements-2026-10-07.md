# Replacement pictures after lengthening an AI pause

Automatic replacement after lengthening is implemented and verified on this
Mac. This record does not close DP-12 or raise the progress estimate; its
remaining implementation gaps are listed below.

## Verified behavior

Extending an accepted Hold beyond its original sampled interval saves the
new duration and captured fallback immediately. The same transaction creates
a durable preparation with the exact Repeat Default/Play address and prior
generation controls. Shortening and re-extending within the interval reuse
the retained pictures.

An exact current request supplies the previous motion, guidance and target.
If that request is stale or absent, including for copied accepted Holds,
the worker recovers those controls from verified immutable artifact provenance.
It checks both host and worker bindings before using them.

The app's existing single AI job thread prepares boundaries and provenance
off the writer. A claim binds preparation, sequence and current revision.
Fulfilment atomically records a normal request and its first attempt; stale
conditioning cannot allocate either. Ready remains separate from explicit
undoable acceptance.

Unrelated edits preserve the intent and requeue against the new revision.
Changed conditioning, deletion, Undo, Discard and explicit superseding
generation invalidate old claims. Redo creates a fresh preparation. Restart
turns an interrupted claim into a visible retryable entry. A missing model,
runtime or conditioning input leaves the saved fallback and an Unavailable
entry in Jobs, with **R** Retry and **D** Discard.

The closed-project `ai-replacements --run` command owns its writer and
processes a bounded batch synchronously. The open app owns automatic
processing. Both use the same preparation, request and attempt records.
Neither path accepts pictures automatically.

Schema 70 deliberately refuses older unused development packages, under the
session's breaking-format authorization. Terminal preparation compaction
retains immutable birth and control proofs for history validation and Redo.

## Verification

Host: this Apple Silicon Mac, macOS 26.5.2, pinned Rust 1.97.1 and the
explicit FFmpeg 8.0.3 development prefix. Scratch evidence is under
`/tmp/deadpan-resume-20261006`.

| Check | Result |
| --- | --- |
| Host/worker control-binding regression | 1 passed, `replacement-model-controls.log`. |
| Workspace tests filtered by `replacement` | 101 passed across 248 targets, `replacement-focused-2.log`. Includes real-media synthetic admission, explicit acceptance, stale-claim rejection and native automatic processing. |
| Workspace tests filtered by `preparation` | 91 passed, 1 failed across 248 targets, `replacement-preparation-1.log`. The failing fixture shortened and restored an accepted Hold before extending it, making the old request stale; it incorrectly expected controls from that request. The accepted-artifact route is required. |
| Corrected workspace `preparation` filter | 93 passed across 248 targets, `replacement-preparation-3.log`. The retention/history target took 80.89 seconds after grouping validation by origin, with additional retired-control tampering checks. The subsequent read-only replay regression passed in the full workspace run. |
| Repository gate, completed in parts | Formatting and both strict Clippy configurations passed. The workspace run passed 4,955 of 4,956 tests; its stale generated command-reference failure passed after regeneration. All 1,047 UI-harness tests and both doc tests passed. Ten workspace and two UI tests retain their existing explicit skips. |
| Independent static review | Claim validity, atomic fulfilment, stale replies, restart recovery, explicit acceptance and CLI/native integration reviewed. Final static review is clear after the corrections below. |

The first focused compile also caught SHA-256 output formatting that no
longer implements `LowerHex` in sha2 0.11; bounded byte formatting fixed it.
Retain `replacement-focused-1.log` with the failed compile.

The queue now reserves worst-case serialized mutable fields on admission and
validates the persisted charge. At the byte limit, tests grow every row's
failure text, revision, isolated node identity, claim sequence and request
identity without losing capacity. Indexed field types and lengths are checked
before extraction. Immutable source request constraints and fulfilment attempt
presence participate in the preparation digest after terminal compaction.

Review also found that explicitly choosing the same fallback must supersede
pending replacement work, including through Redo. The cancellation walker now
runs only in writer command/navigation transactions. Historical birth derivation
remains pure. The final regression leaves a fresh preparation queued after
earlier provider Redos, then checks that full validation through writable and
read-only stores preserves the complete record.

`replacement-preparation-2.log` retained a fixture failure: copying generated
Holds after removing the last current generated provider correctly hit the
generic-ingress guard. The fixture now copies the current accepted artifact
before extending its original Hold; production admission is unchanged.

The gate's initial workspace Clippy run caught a redundant match guard; the
second invocation passed workspace Clippy and caught a missing `HoldVideo`
import in the new replay. The explicit match pattern and import change no
runtime behavior. `replacement-ui-clippy.log` passes the corrected UI target.
`replacement-workspace-tests.log` retains the original command-reference
failure; `replacement-commands-check.log` records the passing targeted rerun.
The generated reference now describes replacement Retry and Discard.
`replacement-ui-tests.log`, `replacement-doctests.log` and
`replacement-final-format.log` complete the gate without repeating passing
workspace tests. Neither interrupted `cargo xtask gate` invocation is itself
reported as passing.

## Real local-model replacement

`replacement-generation-1/summary.json` records 73 passing assertions from
`verify-replacement-generation.py`. A fresh schema-70 project wraps a genuine
Source/Hold/Source definition in two Repeat plays and an outer 2× Retime.
The real local model generated all 30 intrinsic Hold frames for Play 2.
Explicit acceptance isolated that play; Undo/Redo preserved the immutable
request origin and mapped current address.

Lengthening the accepted Hold to 36 frames immediately saved its captured
background fallback and one durable replacement. An unrelated rename retained
the intent at the newer revision. The closed-project runner fulfilled it and
the real model reached Ready with the same scope, Subtle motion and independent
scope clock advanced once. Ready left timing, history and fallback unchanged.
Explicit acceptance preserved silence and the shared Default/Play 1 subtree.
Undo/Redo restored the longer fallback and accepted replacement respectively.
The final MP4 passed the production publication verifier; rendering left the
document unchanged.

This was a development-host run using immutable private binary copies and the
existing verified 36.2 GB pack. The app was built with `ui-harness`, but executed
its production headless path and real backend. Runtime `0.15.8+deadpan5`, host
profile 8, context 5 and retained boundary hashes matched. The run preserved
the real HOME and used explicit development runtime paths. It establishes no
packaged-runtime or perceptual-quality claim.

Initial generation took 87.206 seconds, replacement 131.729 seconds, and verified
Render 41.215 seconds. Visual replays and part of the release build ran
concurrently, so these are elapsed observations, not performance qualification.
The complete run took 263.515 seconds.

| Object | SHA-256 |
| --- | --- |
| App | `6be34214ec826780840956b630de6cd67e9ec5fcc60042d4fafa4b838f2f0f96` |
| CLI | `4d33a52b22c2e18186f7ed1282632341af5638c242248886ae0ca759f1b42c3b` |
| Original fixture | `5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918` |
| Verified MP4 | `ed4203a6b3d9aa1a9b30dbc25b06c9557c2f3bf3e9200d7a132d359c6d56be39` |

## Release packaging

`bundle-replacement.log` records the 726.6 MiB ad-hoc signed app, including
its private AI runtime. All 74 Mach-O files passed the load-reference audit.
`bundle-replacement-verify.log` passes relocated, scrubbed-environment
startup/shutdown, export, helper/runtime checks and all damaged-bundle refusals.

The current release bundle also reopened the real replacement project and
published `replacement-packaged-export/replacement.mp4`. The production verifier
confirmed generated pictures in the output, and the complete authored document
remained equal to the development run's final document. This export required
no model installation. `replacement-packaged-summary.json` retains its receipt
and identities:

| Object | SHA-256 |
| --- | --- |
| Release app | `c0b935deb5c727b0149f66a252e0c3e09147bccc6dae2dcd3ec405d9cad47044` |
| Release CLI | `2ee57c657e07cdfcc4c8d42d40415fac1bfc3b94cd2fbb4b6b45e4ebfa10155d` |
| Verified release MP4 | `36585a6f5853ba86cba15d768b9107982e8bf40b1d7fef126941bef173ff5b0a` |

## Native replay inspection

The direct `ai-replacements` replay passed all 14 scenario checks plus the
Kestrel routing audit in `replacement-ui-focused-1`. Production commands insert
12 frames, explicitly accept a qualified synthetic candidate, extend to 18
frames, observe a scripted missing-model result, then Retry and Discard using
the production Jobs keys. Timing, exact target and durable cancellation survive.

The six-frame contact sheet and full-size `ai-replacements-037.png` and
`ai-replacements-039.png` were inspected at 1280×820 against the workspace
design target. The retained picture, lavender selection, separate Original/edit
clocks and compact footer remain legible. The Jobs row wraps within its panel;
Retry, Discard and Close are visible with their key labels. Layout warnings
describe settled retries with distinct causes, not ignored or repeated
unresolved retries. No live screenshot was taken.

Two small wording defects were observed: a failure reason ending with a period
produces doubled punctuation in the footer, and dispatch messages retain a
progress ellipsis after the row has finished retrying or disappearing. They are
included in the next native timing-display patch; the saved-state checks pass.

## Verification limits

Synthetic worker tests establish orchestration and media admission, not
real model behavior. Offline GPU captures establish painted layout and
production input routing, not physical input, VoiceOver speech or perceptual
quality. Those owner-only limits remain governed by spec §29.1.

## Full replay run and corrective coverage

`replacement-replays-1.log` retains the original full invocation: 64 scenarios
passed, four failed, and two explicitly require separate project fixtures
(`ai-pause-ready` and `generated-picture`). It ran 7,128 checks in 2,494.4 seconds
with three jobs. It is not a passing invocation.

Inspection of the failure reports and their full-size captures found four
outdated harness assumptions:

- **Retime:** the replay clicked a speed control below the visible inspector.
  It now scrolls with real wheel input and verifies the complete button is
  inside the paint clip before clicking. The shared Camera reveal helper also
  handles a retained scroll offset with a target above the viewport.
- **Render:** command admission became idle before the reopened workspace reply
  was published. The coalesced-session witness now captures the actual new
  workspace, then the matching workspace/render update, before releasing that
  exact reply to the UI. It no longer races unrelated owner publications.
- **Accessibility:** the Help assertion expected an obsolete instruction line.
  It now requires the displayed search, scroll and close instructions, retaining
  the Polite live-region assertion.
- **Full session:** it required `:preview-ai` below the inspector viewport while
  `,x` was visibly taught in the inspector and footer. It now uses that genuine
  keyboard preview path and retains the candidate and acceptance assertions.

Independent static review is clear for all five changed harness files. The
corrected UI-target strict Clippy check passes in
`replacement-replay-fixes-final-clippy.log`. These corrections do not change
production behavior. All 50 source hashes in `replacement-source-final.json`
still matched after the real-model run, release bundle and performance runs;
the five corrective harness files are additional source changes.

`replacement-replay-fixes-1.log` passes all five affected scenarios: Camera
(16 checks), Retime (20), Render (114), Accessibility (40) and full session
(133), including each routing audit. The 323 checks took 247.7 seconds with
three jobs. Its app hash is
`9ad6c36d0b60bdcb8ac4a05c95004076cd8a0bab5c6e472e1508d3713c0a4d04`.
The other original passes remain applicable. Together these runs complete
the 68 runnable scenarios; the two explicit fixture skips remain skips.
The real-model and packaged export evidence above covers its own exact scope
and does not claim those skipped replay scenarios ran.

The corrected Retime and Camera entry captures were inspected at full size:
the revealed controls, command preview and framing controls are legible, with
the existing selection and focus cues preserved. The original failure capture
already showed the Help instructions used by the corrected live-region check.
The corrected full-session preview capture was also inspected: the candidate
is visibly unsaved, and the footer teaches comparison, audition, acceptance,
discard and return to the edit without covering the main editing keys.
All replay layout warnings settled; no ignored or repeated unresolved layout
retry was accepted. No new production behavior was added to make a replay pass.

## Separate release performance

`replacement-performance-1` contains four sequential release-mode runs after
the model, packaging and full visual suite had finished. No other goal build or
media job ran concurrently. Each passed: `ai-replacements` (14 checks),
`rapid-input` (329), `edit-latency` (450) and `large-project` (526), plus one
Kestrel routing audit per run. The reports retain all raw samples and
`summary.json` collects their distributions; `runs.json` records exit codes,
elapsed time and host load. One-minute load before the runs was 2.13–3.46.

The executed release app SHA-256 is
`236027fae87da4887910c874d78152b0e44c05befa302a2c4d9316d095e42b3b`.
The exact Cargo artifact inventory and the three adjacent helper hashes are
retained in `replacement-performance-artifacts-all.jsonl` and
`replacement-performance-bin/identity.json`. These binaries precede the five
harness corrections above; none of these four performance scenarios uses the
corrected branches. The production source is identical.

| Workload | Samples | p95 | Maximum |
| --- | ---: | ---: | ---: |
| Warm navigation input CPU | 120 | 0.243 ms | 0.247 ms |
| Warm navigation to completed GPU picture | 120 | 1.345 ms | 1.490 ms |
| Cached Repeat to completed picture | 40 | 9.514 ms | 9.837 ms |
| Hold insertion to fallback picture | 40 | 14.916 ms | 15.011 ms |
| Navigation among 10,000 root beats, CPU | 160 | 0.550 ms | 0.762 ms |

These sampled warm workloads meet their respective 8 ms input, 80 ms navigation,
50 ms Repeat and 100 ms Hold budgets on this Mac. The AI replacement scenario
also passed, but its six mixed picture samples are too few to qualify a separate
replacement latency distribution. Broader mixed samples include initialization
and cold work: the rapid-input scenario's maximum input-to-picture time was
215.837 ms. It must not be reported as a uniformly sub-80 ms run.

The same real frame fixture hash listed above was used. Initial import/index
was cold and subsequent operations warm; OS file cache, power and thermal state
were uncontrolled. No GPU readback or screenshot capture occurred in measured
runs. Physical display latency and broader hardware/performance coverage remain
outside this measurement.

## Remaining DP-12 implementation

An independent audit against the normative spec identified further work beyond
this replacement milestone:

- `,a` must insert the same counted pause as `,h` and request a candidate (§7.5).
- `:hold-provider ai` must request pictures for an existing Hold (§31).
- An explicit keyboard action must revert accepted pictures to their captured
  fallback after intervening edits (§12.6).
- Changed source boundaries must trigger replacement with preserved controls,
  rather than only making the old request stale (§12.7).
- One-sided generation must work at an end boundary (§12.2); current bridge
  conditioning requires both endpoints and rejects extension modes.
- The native inspector must report material conversion between the requested
  Hold interval and the model's native interval (§12.4). The planner and CLI
  already retain those values.

DP-12 stays Partial. These implementation gaps remain separate from the
owner-only quality and physical-input checks.
