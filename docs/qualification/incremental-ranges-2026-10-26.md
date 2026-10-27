# Native incremental analysis ranges, 2026-10-26

This is a continuation of the earlier [incremental audio analysis
record](incremental-audio-analysis-2026-10-26.md). It records the later native
integration and BLAKE3-addressed cache witness. It is a partial DP-10.7
implementation, not completion of analysis quality, latency, packaging or the
broader Deadpan requirements. It includes real-worker, actual codec and native
offscreen Metal evidence, with separate claims for each.

## Implemented

The native app now uses disjoint 24-second Original audio ranges with 2.4
seconds of inference context on each side for Whisper and detection-only VAD.
The latest visible Original or Edit picture PTS sets the next range priority;
a bounded range already running completes before that priority changes.
Transcript and activity ranges share one inference admission, released after
each range. Cached results launch neither worker. Complete transcript and
activity objects are assembled and saved only after full nonoverlapping source
coverage. Partial words appear on the rail with explicit incomplete coverage
and can jump the cursor; they do not enable search, corrections or editorial
word/pause objects. The initial `AudioSession` still decodes the full audio
stream, and the CLI `transcribe` command retains its whole-input path.

The native app's shot detector also uses cursor-priority disjoint ranges,
normally 2,048 owned pictures per range (widened when needed to bound range
bookkeeping), with a 50-picture comparison halo on each
side. Only the owned measures count toward coverage. A complete shot analysis
is assembled and published after every source picture has one owned measure.
The CLI's sequential resumable shot scanner remains separate.

Transcription, detection-only activity and native shot ranges use the private
`AnalysisRanges` directory. Cache filenames are BLAKE3 hashes of each exact
serialized range key; each record payload is checked with SHA-256. The key
includes source identity, range implementation and owned/context bounds, and
the audio key additionally includes stream timing, model/detector, language,
protocol and helper identity. The cache is rebuildable and bounded; a damaged
or unavailable entry is reanalyzed. This does not add algorithm provenance to
the existing complete SQLite transcript or activity keys.

## Checks and review

Focused evidence available for this continuation:

- The Media picture-range suite passed both tests on its second run, including
  parity with whole scans across CFR/VFR and interpreted inputs. The first run
  exposed an out-of-range request on a one-picture rotated fixture; the test's
  requested window was corrected without changing the fixture,
  and the second run passed.
- The legacy CLI shot suite passed all five tests. The new CLI shot-range suite
  passed eight tests after adding early complete-picture admission. The app's
  existing shot/save regressions pass six tests. Neither result is an accuracy
  corpus qualification.
- Four CLI range-cache tests passed, covering the exact BLAKE3 key address,
  private-directory and file checks, checksum rejection, and nonblocking lock
  behavior.
- Four partial-rail binary tests pass, including identical-word identity,
  prepended words, fractional terminal coverage and old-attempt isolation.
- The final tenth native replay passes 70 transcript assertions and the one
  Kestrel-shortcuts assertion (71 checks total). It checks exact cursor jumps,
  incomplete coverage, same-version correction errors, complete compact word
  hit targets, retained focus after earlier-word publication and Enter, plus
  the existing full transcript/search/editorial scenarios. Delivery of partial
  words is explicitly synthetic over the real fixture; it is not an inference
  witness. Production Metal is offscreen, with simulated device output and
  accessibility/kittest input, rather than a physical native window.
- Final eight-crate all-target strict lint, including the app's ui-harness
  feature, passes with warnings denied. Relevant source guards remain unchanged.

Independent review found and corrected issues in partial correction authority,
inference-slot release before the next range, layout focus, stable partial-word
IDs and early shot-range admission bounds. The strengthened ninth replay
exposed that egui push_id still mixed UI position into auto-widget identity;
explicit stable scopes fix it, and the tenth replay passes. A replay fixture
compile error and earlier layout/test-assumption failures remain in the
receipts. No pristine-HEAD build was run: the shared checkout contains
uncommitted Core/Plan/Audio sound work. Runs used recorded source inventories
and guards; they are not a clean-tree comparison.

## Local real-worker witness

The retained `incremental-real-blake3-first` witness used the actual native
Whisper helper, the installed base.en model, pinned Silero v6.2.0 and a repeated
synthetic-speech video. Its guarded receipt reports 47.633 seconds overall.
After the first run, reading all four ranges from cache took 9,100 ms and
admitted zero model workers; the cached transcript and activity matched, and
the authored project revision did not change. The run still includes the
full-source audio decode. These are results from one local synthetic fixture,
not a speedup or latency qualification and not a speech-accuracy result.

The retained historical SHA-addressed witness is described in the
[earlier record](incremental-audio-analysis-2026-10-26.md); its timings are not
the BLAKE3 run reported here. The newer run's evidence inventory is retained at
[`tools/model-qualification/evidence/2026-10-26-incremental-ranges/inventory.json`](../../tools/model-qualification/evidence/2026-10-26-incremental-ranges/inventory.json).
It contains 127 payloads (10,163,125 stored bytes), plus the inventory and its
digest. Inventory SHA-256:
fadca366ee4288cb2c80c229496a82bd2b09a5c7bbe83f80e47b9ae0034ff111.
Every stored/expanded size and hash was verified after writing. The final
native executable SHA-256 is
e9794438a668e946a222295727fc06ec0703fd013bce206d8ba0fc7868ca13d8.
The exact source-guarded build receipt is native-incremental-build-eleventh;
the actual BLAKE3 example has its own build receipt and executable identity.

## Still open

- Accuracy on varied real speech, transcription and VAD quality, and the
  partial rail's behavior beyond the recorded synthetic interaction.
- Long-media resource and latency measurements. The initial full audio decode
  remains, and the 9,100 ms cached run is not an end-to-end latency target.
- Full native project-open and packaged-app qualification for this integration.
- CLI transcription range integration; CLI `transcribe` still analyzes its
  whole input. Tracking and local refinement service integration are separate
  work and are not qualified by these range checks.
