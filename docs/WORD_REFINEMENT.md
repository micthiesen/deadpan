# Local word timing proposals

Current status: the English workflow is implemented and locally verified through
native review, real device audition, explicit acceptance/Undo, offline installation
and a relocated signed bundle. [Qualification](qualification/word-refinement-2026-10-25.md)
closes DP-10.3's local scope. Broader DP-10 work and the older native-helper trust
gap in DP-22 remain open. Experimental precision/coverage failures stay failed.

The normative contract is specification §9.3/§11.2: approximate annotations,
waveform adjustment, explicit acceptance and preserved manual timing. A model
posterior measures support on a forced path. It is not a probability that an
edge is correct. Existing cuts and authored history never move when analysis
is proposed or corrected.

## Timing and acceptance

`deadpan_analysis::refinement` aligns unchanged lexical text through a bounded
pronunciation DAG and CTC path. The frozen context guard checks unexplained
supported phonetic events. Unavailable words retain raw diagnostic spans and
reasons. Pronunciation alternatives use polynomial work rather than enumerating
every sentence pronunciation tuple.

`RefinementCapture` freezes the raw recognizer output, corrected transcript,
correction regions, selected word indices and complete measured analysis extent.
It keeps complete selected and neighboring segments, refusing overlapping seams
or contexts beyond 64 words / 30 seconds. It never truncates speech to fit.
The typed global 16 kHz offset is added before converting model intervals to
stored centiseconds. Starts floor and ends ceil explicitly. Model frames are
20 ms; the annotations remain approximate even when their coordinates are exact.

Only explicit `RefinementReview::accept` returns a correction. It rejects stale
raw/corrected analyses, unavailable offers, pinned words, neighbor crossings and
incidental changes to unselected words or earlier correction regions. The result
is one correction Undo step, independent of document revisions. A lifecycle guard
runs at admission and immediately before SQLite commit; rejection rolls back the
value, version and both correction stacks.

The shared CLI preparation helper reads one strict transcript-selection snapshot,
opens the qualified Original, prepares its exact-phase window, runs the isolated
worker and rechecks the revision and analysis guard. Initial audio-session opening
still builds the full stream's private PCM cache. This is not incremental selected
range decoding. All decoding, hashing, dictionary parsing, inference and full
snapshot cloning belong to a preparation worker.

## Native backend and pack

`native/deadpan-align` implements deterministic 16 kHz feature extraction and
CPU-only ONNX Runtime 1.31.0 inference. It uses at most four inference threads.
The original alpha-0.6 checkpoint is retained as data: a compiled fixed range
table copies 24 exact tensors into a zeroed graph template. Whole-checkpoint,
per-range, template and final graph hashes are checked. No archive metadata,
pickle, Python, Torch or downloaded installation script executes in the app.

- Original checkpoint: 57,562,900 bytes, SHA-256
  `4070c3b89fb27129e2999f550ddb55c747ec77da843fa8a520989b8c5d09723f`.
- Assembled graph: 19,219,161 bytes, SHA-256
  `fe1e1890f968e7c19c0d4dde3ac7e15dfca15fd3bf2f9d5c41b2a5d1f120c57d`.
- `phoneme-ctc-en` version 1: six original-source files, 61,188,767 bytes,
  including statistics, CMUDict data and separate source/data notices.

The worker consumes only verified PCM, checkpoint and statistics. Text and
correction ownership stay in the host. Protocol 1 binds request, attempt,
cancellation token, input size, runtime pins, tensor shape, exact artifact bytes
and output scope. Input is 401–480,000 finite mono samples; output is at most
1,501 × 41 float32 log probabilities. The host admits a contained hashed snapshot
only after clean process teardown. EOF, malformed control or a second work request
revokes ownership; an unrelated valid cancellation does not cancel another job.

The original pack activates only after full dictionary parsing and actual native
inference with independent numerical smoke anchors. Offline import/export uses
the same verified pack path. Signed metadata updates cannot replace components
that the compiled assembly and preprocessing have not qualified.

The author repository's BSD-2-Clause source notice does not establish a separately
explicit model-weight grant. The pack records unspecified checkpoint/statistics
terms and no redistribution permission. Original CMUDict terms remain separate.
The app bundle retains the graph-source, Unicode and Python data notices, plus
ORT's MIT and third-party notices. No Python runtime is shipped for refinement.
See [packaging](PACKAGING.md) for the explicit qualified ORT prefix and relocation.

## Verification and limits

Permanent [evidence](../tools/model-qualification/evidence/2026-10-25-word-refinement/)
and its SHA-256 inventory retain the final workflows. Scratch evidence:
`/tmp/deadpan-word-refinement-2026-10-25`. It retains the frozen
model experiments, source snapshots, exact invocations, failures, streams and
binary identities. The permanent inventory excludes model weights and raw learned normalization statistics.

The Rust port preserves every label path, pronunciation, interval, offer/refusal
and context-guard structure in all 24 observed development cases. Maximum feature
absolute error is 0.000355005; maximum emission error is 0.000228882. These are
numerical portability measurements, not human boundary accuracy. Actual supervised
worker cases also pass, including cancellation, EOF, corrupt inputs and FIFO/
symlink refusal. A real qualified Original test verifies the analysis clock,
unchanged authored state and pinned-word refusal. Its tone fixture establishes
wiring, not speech quality.

The [earlier experiment](../tools/model-qualification/word-refinement/plan.md)
failed its stricter coverage and foreign-energy targets. Those failures remain
failed. These 24 observed cases are development/parity data, never an untouched
final split. Human phonetic acceptability and listening judgment remain owner
verification under §29.1, with concrete steps in the
[owner list](REQUIREMENTS.md#to-verify-owner). No automatic precision, conversational accuracy or release
readiness is claimed by the port checks.

## Use

Install the English phoneme pack through Models, open corrections, select an
unpinned recognized word and activate Refine word timing locally. The review
shows Approximate / Unsaved, measured waveform, current edges and proposed edges.
Space hears one fixed comparison context; Tab retains native button activation.
Accept creates one independent correction Undo step. Manual adjustment remains
available. Non-English cached transcript keys refuse before model loading.

The headless path uses the same preparation and guarded write:

    deadpan-cli refine-words PROJECT.deadpan --word 0 --models-root PRIVATE_MODELS
    deadpan-cli refine-words PROJECT.deadpan --word 0 --models-root PRIVATE_MODELS --accept PROPOSAL_ID

Acceptance reruns inference and refuses changed analysis or positions. The
packaged worker/library checks use independent compiled content pins and valid
signatures. Removing its package marker or supplying an unpinned host refuses.
These checks are specific to alignment; the older native-worker paths remain
open. Initial source-cache construction and per-job model loading are not
incremental decoding or warm model reuse.
