# AI pauses inside Repeat and Retime, 2026-10-07

Generation and acceptance capture a Hold's full authored address: its node
and every enclosing Repeat Default or stable Play choice. A 30-frame Hold
generates 30 pictures even when an outer Retime presents only 15 of them.
Accepting into one play isolates that branch atomically; accepting into the
Default retains existing overrides. Background completion never changes the
committed provider.

## Definition clocks and durable addresses

The conditioning root is the highest ordinary Sequence containing the Hold
below its nearest Repeat/Retime ancestor, or the project root. A local Hold
interval `[s,e)` uses boundary samples `s - 1/2` and `e + 1/2`. Missing local
boundaries refuse generation. A visible occurrence, outer neighbor or preview
proxy cannot substitute for them. Dormant Defaults remain eligible when both
local boundaries exist.

Context 5 retains the exact rational positions, project, origin revision and
definition. Host profile `deadpan-ffv1-bridge-8` and worker
`0.15.8+deadpan5` bind that origin and the `N+1` boundary distance. Historical
Project-clock contexts and accepted profiles retain their original evidence.
Weights, prompts and quality thresholds are unchanged.

Store schema 69 introduces independent monotonic generation scope clocks.
The immutable worker binding and origin address stay fixed; only the current
address follows isolation maps rederived from the actual validated history
command. This includes a sibling edit that clones a Hold indirectly. Undo and
Redo move addresses without reviving stale requests, attempts or discarded
media. A changed audit digest requires chronological reconstruction; forged
events and valid-looking sibling retargets fail validation. The scope index
has independent count and byte bounds. Earlier unused development packages
are refused under the session's breaking-format authorization.

Each reconciliation prepares one render plan for its exact borrowed proposed
document. It cannot reuse a proposed revision ID as a cache key: a failed
write leaves that ID available to a different edit. The regression exercises
different candidates with the same revision and verifies independent Play,
Default and sibling relevance. Immutable stored origin plans remain cached.

## Native workflow

The nested inspector exposes generation controls, full intrinsic picture
count and definition-clock join readings. Jobs, variants, retries and
interrupted-job labels retain their complete authoring scope. CLI `--scope`
uses the same strict typed address; live job status returns it.

Preview and audition additionally capture a concrete occurrence and its full
root-frame interval. A scope change revokes pending previews and cached
comparisons. Acceptance returns the remapped inspector target so navigation
continues in the isolated play. Several selected plays currently refuse
explicitly; the shipped bridge provider also refuses missing endpoints.

## Verification

Apple M5 Max, macOS 26.5.2. Source parent `acb1a1c4`; retained evidence is in
`/tmp/deadpan-resume-20261006`.

- Core/plan and store tests cover exact definition lookup, dormant Defaults,
  deep scopes, count-independent work bounds, acceptance, sibling isolation,
  atomic rollback, Undo/Redo, reopen and forged scope history.
- Real-media conditioning covers Repeat Default/Play and Retime with identical
  raw boundary PNGs, full intrinsic duration, exact source PTS and missing-edge
  refusals. Native service tests exercise real synthetic-candidate preparation,
  scoped acceptance and stale preview rejection.
- `scoped-host-lib-4.log`: 517 library tests passed; four explicitly ignored
  checks retain their existing separate qualification requirements.
- `scoped-python-1.log`: all 84 worker tests passed.
- `scoped-replays-1`: all 77 checks passed across `ai-scoped`, `ai-variants`
  and `ai-compare`. The scoped scenario uses production keys through Generate,
  Preview, Play navigation, Accept and Undo/Redo. Inference is synthetic; the
  scenario opens no audio device. The replay app SHA-256 is
  `6420406e80a67918fe14a2d91a35e78c3312c249f04201137eefd40453c2d204`.
  Offline capture `ai-scoped-076.png` was inspected at 1280×820. Settled layout
  retries had distinct causes; no ignored retry or repeated unresolved cause
  was painted. No live screenshot was taken.

Independent review found the repeated after-plan compilation cost. Review of
the first cache fix caught the unused-revision retry hazard; the final borrowed
transition preparation and regression address both.

### Repository gate

The gate completed in parts. `scoped-clippy-5.log` passes strict workspace
Clippy; `scoped-gate-finish-2.log` passes formatting, strict UI-harness Clippy,
all 1,039 UI-harness tests and both doc tests. Ten workspace and two UI tests
remain explicitly skipped by their existing qualification configuration.

`scoped-gate-finish.log` ran all 4,931 workspace cases: 4,929 passed, with two
test failures. The first still expected schema 66 to migrate; its replacement
verifies truthful refusal, identical database bytes/version and no backup.
The second mistook asynchronous startup backup-settings loading for a remote
macro publication. Setup now consumes that initial reply; the strict
no-publication, unchanged cursor intent and durable-state assertions remain.
All eight affected backup/macro cases passed on rerun. The UI gate also ran
both corrected tests. No production change was needed for those failures.

Earlier integrated runs also exposed two stale context fixtures, a doctor
schema assertion and old boundary-field uses in a native integration test.
The corrected fixtures bind their real context/region evidence and pass. A
new exact-coordinate error payload was boxed to keep host error sizes bounded;
the remaining lint fixes change no behavior. Initial failures remain in
`scoped-host-tests-3.log`, `scoped-gate-1.log` and `scoped-clippy-{2,3,4}.log`.

### Packaged real-model run

`bundle-scoped.log` built the 725.9 MiB ad-hoc signed app and audited all 74
Mach-O files. `bundle-scoped-verify.log` passes relocated, scrubbed-environment
startup/shutdown, helper and runtime checks, verified export and damaged
helper/runtime refusals. The approved 36.2 GB model pack imported into a fresh
scratch home in 12.202 seconds and passed its bundled smoke test.

`scoped-generation/summary.json` records all 60 passing checks from
`verify-scoped-generation.py`. The fresh project contains a Source/Hold/Source
Sequence inside Repeat 2 and an outer 2× Retime. Explicit Play 2 generation
became Ready in 76.383 seconds. It retained all 30 intrinsic pictures while
the root presents 15 frames, `[105,120)`, sampling master ordinals
`1,3,5,…,29`.

Context 5 retained local boundary positions `119/2` and `181/2`, exactly 31
frames apart, tied to the original project/revision and `scoped-local`
definition. Retained context and both PNG hashes matched their declarations.
Host profile 8 and worker `0.15.8+deadpan5` agreed on the exact immutable
request binding. Ready left the authored fallback unchanged. Acceptance
isolated Play 2 and preserved the complete shared Default/Play 1 subtree.
One Undo restored the fallback and original request address; one Redo restored
the isolated provider and mapped address. Verified MP4 publication took
2.420 seconds and left the accepted document unchanged.

The source/configuration manifest `scoped-source-before-bundle.json` retained
86 hashes against parent `acb1a1c4`; all matched after qualification. Executed
release SHA-256:

| Executable | SHA-256 |
| --- | --- |
| `deadpan-app` | `5a8ec15d4d456f413b5b68dea8256988aea2e3bafbc29953b0df1a5307541858` |
| `deadpan-cli` | `f5ccd62b81b466957325a5cc800a41284c48b143ef944a587a924abc145a34e5` |

The synthetic Original fixture SHA-256 is
`5a820a79bf550d484d8ecb37ffddd048d0636794ebce5db83e4f5ce5f5e64918`;
the verified movie SHA-256 is
`825b06f789b4db81bbf09c5967378f4d98f5d9a59cac6155232370ff0a3b1d64`.
This verifies the real pipeline and structural clocks, not perceptual quality
on real-person footage.

## Remaining AI work

Spec §12.7 requires a replacement request when an accepted Hold is lengthened
beyond available generated handles. The current reducer restores the captured
fallback, but the host still requires another explicit Generate and can lose
the old controls once that request becomes stale. Queueing the replacement
with its previous motion, guidance and target is the next implementation gap.
Corpus calibration, listening and real-person identity/seam judgment remain
on To verify (owner) under §29.1.
