# Normal AI extension workflow, 2026-10-08

The native app and headless commands now resolve Bridge or Extension from the
captured Hold definition, select an independently admitted model pack, and run
the shared preparation, supervised attempt, qualification and acceptance path.
Production packaging and all eight real duration/direction cases below pass,
including explicit acceptance and verified portable export without the model.
DP-12 remains Partial for resident model reuse, fixed text caching and the
remaining model/performance comparisons.

## Implementation

- `:generate [N] mode=auto|bridge|extend-left|extend-right` and CLI `--mode`
  select the operation before decoding conditioning media. Auto uses Bridge
  with both definition endpoints or Extension from the available side. Explicit
  Extension never changes direction to satisfy a missing anchor.
- The independent `ltx-2.3-q4-extension` version 1 manifest admits K9 context,
  E8 through E72 in steps of eight, 24 fps, 768×320, at most three authored
  seconds and at most 180 project frames. Nearest legal generated duration wins,
  with shorter ties. Sampling reads only generated pictures.
- Runtime `0.15.8+deadpan6` retains Bridge provider
  `0.15.8+deadpan5` and adds Extension provider
  `0.15.8+deadpan-extension1`. Both pack installations must pass their own
  operation-aware smoke test before activation.
- Prepared inputs and attempts carry tagged plans. The host snapshots all
  temporal inputs before launch, rejects cross-operation completions, confirms
  worker cleanup, qualifies outputs and publishes an immutable Ready bundle.
  Ready leaves authored state unchanged. Acceptance is explicit and undoable.
- The inspector distinguishes the requested preference from the resolved
  operation and labels the opposite seam as unconditioned or absent. Current
  jobs retain Auto through more variants; reopened controls show their saved
  resolved mode. Unavailable generation preserves earlier Ready candidates.
- Lengthening an accepted Extension saves the longer fallback and starts a
  replacement in its captured direction. The replacement requires acceptance.

## Automated verification

Apple M5 Max, 128 GiB, macOS 26.5.2; Rust 1.97.1 and pinned FFmpeg 8.0.3.
Scratch evidence: `/tmp/deadpan-extension-native-20261008`.

The workspace gate completed in parts: 5,429 of 5,431 tests passed in the broad
run, and its two stale-reference failures passed after correction. All 1,080
UI tests, both doctests and 123 Python tests passed. Existing ignored tests
remain explicit (10 workspace, two UI). Formatting and strict workspace/UI
Clippy passed. The final retained-mode regression also passed.

Five targeted replays passed 128 checks, including their Kestrel routing audits:
`ai-pause`, `ai-variants`, `ai-extension`, `ai-compare` and `model-packs`.
The new Extension replay covers real temporal capture, host qualification,
publication, Ready without an edit, preview, explicit acceptance and Undo.
Its output comes from the scripted V3 worker. The replay reports retain that
limit, scripted picker selection and untested physical input/display/audio.
The rendered Ready inspector was also inspected.

The first UI run emitted one nextest pipe-closure warning on the unrelated
pure gain test `invalid_edits_are_atomic_and_range_changes_do_not_drop_hidden_keys`.
Its assertion passed, and the isolated follow-up passed without a pipe-closure
warning. This does not establish the cause of the broad-run diagnostic.

## Production qualification

The real matrix uses two committed, synthetic nine-picture Originals from
[the one-second qualification](extension-one-second-2026-10-08.md). Each starts
as a normal one-Original project with a silent fallback at its definition edge.
Both directions are prepared at 0.5, 1, 2 and 3 seconds, with seed 42107;
motion is Still for 0.5/1 second, Subtle for 2 seconds and Moderate for 3 seconds.
The expected generated counts are E8, E24, E48 and E72 respectively.

All cases use ordinary packaged `generate-hold --mode auto`, with no developer
runtime override. The host admits each immutable Ready bundle and leaves the
authored document, including its fallback, exactly unchanged.

| Authored pause | Direction | Generated pictures | Wall seconds | Worker peak RSS, GiB |
| --- | --- | ---: | ---: | ---: |
| 0.5 s | From left | 8 | 156.002 | 13.502 |
| 0.5 s | From right | 8 | 210.209 | 13.287 |
| 1 s | From left | 24 | 331.637 | 13.502 |
| 1 s | From right | 24 | 324.207 | 13.513 |
| 2 s | From left | 48 | 507.101 | 13.517 |
| 2 s | From right | 48 | 501.665 | 13.256 |
| 3 s | From left | 72 | 703.957 | 13.282 |
| 3 s | From right | 72 | 685.344 | 13.252 |

The independent integer RGB oracle checks all 230,031,360 channel values across
312 sampled pictures, with zero context-picture fetches. Every source latent
matches its preserved context exactly. Full worker and host media checks pass.
Explicit acceptance changes only the Hold provider and its two generated asset
registrations. Cold reopening, Undo/Redo and full store validation pass for all
eight cases.

Each accepted project is copied portably and reopened in a sandbox denying the
source package, installed models, development model cache, bundled AI runtime
and outbound IP. Positive and denied file/network probes verify those barriers.
Ordinary Render completes full emitted-file verification; separate preview/export
picture samples and silent PCM windows also pass. These fixtures have no source
speech, so the silence checks do not measure audible A/V synchronization.

System samples taken every 15 seconds report zero swap throughout. `pmset`
reports no recorded thermal/performance warning; this is not a physical thermal
test. The minimum reported system free percentage is 69%. RSS and the retained
end-stage MLX counter do not establish peak unified-memory pressure and must
not be added together. Each case uses a fresh worker on a shared Mac with an
uncontrolled OS file cache. One attempt per case does not establish warm-model,
p50/p95 or interactive editing performance.

Motion estimates are available for 23 of 47 pairs in the two-second left case
and 47 of 71 in the three-second left case. All other pairs explicitly report
unavailable coverage. The synthetic fixtures contain no detected face or
selected region, so their face, mouth and region measurements are unavailable.
The opposite seam is absent at these definition edges. Earlier controlled
pixel/geometry fixtures retain their separate coverage; this matrix does not
establish real-person quality or threshold calibration.

The packaged native app also reopens the accepted three-second right extension.
Accessibility inspection confirms its saved direction and Moderate motion;
Generation controls focuses `generate mode=extend-right motion=moderate target=none`.
Escape returns to Normal, the document matches its retained JSON receipt,
and Command-Q exits successfully. No screenshots or new inference
were used in that check. Expanded timing details below the returned accessibility
viewport were not inspected.

## Retained evidence and identity

[Evidence](../../tools/model-qualification/evidence/2026-10-08-extension-native/README.md)
retains command results, failures, source inventory, bundle provenance, replay
checks, all eight qualification envelopes, exact oracle results, system samples,
acceptance/export receipts and the native accessibility observation. Generated
media, model weights and executables stay in the recorded local scratch paths.

The final CLI SHA-256 is
`760e08b9c505a42aa8206da323a04c18f24b8bab8194534c9b451859e96677cb`.
The source inventory is based on `a3e2e00b` plus the retained implementation
changes. Separate subsequent disk-full tests and read-only Render reply fixes
are not part of this executed bundle. The native app SHA-256 and these limits
are recorded in `native-ax.json`.

## Retained failures

- The first broad gate found stale generated command documentation and an
  adversarial model-catalog seed count. Both references were corrected and
  their tests rerun successfully.
- The first replay batch found an offscreen model installation button, loss of
  the current Auto preference after Ready, and a model-catalog refresh skipped
  for unresolved Auto. Native focus now reveals the button, current jobs retain
  their preference, and both offers refresh the catalog. All three replays
  pass after the fixes. A new replay label comparison had a Rust string-type
  error during compilation; it was corrected before the successful rerun.
- Independent review found that Generate another reconstructed a resolved
  mode even when the current job retained Auto. The service now reuses only
  same-revision, same-target controls matching the current request identity
  and constraints. The retry/reopen regression passes, and review is clear.
- The first relocated bundle built, signed and passed dependency audits,
  import/render, runtime startup and tampering checks. Both model imports
  refused activation because the strict Rust smoke reader omitted the Python
  report's `adapter_sources_sha256` field. The subsequent unavailable doctor
  result was a consequence of those refused installations. The reader now
  requires all seven adapter filenames and typed SHA-256 values without
  loosening the closed schema. All 18 focused runtime/model tests pass,
  including complete and malformed reports and the real network-denial smoke
  fixture. The corrected bundle passes all relocated positive and negative
  checks, including both pack imports (12.4 seconds Bridge, 12.7 seconds
  Extension), private runtime startup, ordinary import/render and damaged-file
  refusals. Normal Extension installation also passes (12.5 seconds).

## Remaining AI scope

The worker still reloads its model on every attempt. A resident model within
a measured memory budget and fixed text-embedding caching remain unimplemented
(§13.5). The distilled 2B macOS/MPS comparison, MLX precision comparison and
full cold/warm interactive performance measurements (§13.4) remain open.
The 60-second target is provisional, and the pack is not labelled Fast.
Generation and acceptance handle Default or one stable Play per request;
multi-play batching is unavailable and is not a separate §12 requirement.
Human quality and threshold calibration, listening, physical input and other machines remain on
[To verify (owner)](../REQUIREMENTS.md#to-verify-owner).
