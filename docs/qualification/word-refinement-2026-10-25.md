# Local word-timing proposals, 2026-10-25

The English phoneme workflow is implemented and locally verified through native
review, real CoreAudio delivery, headless explicit acceptance, offline model
installation and a relocated signed bundle. It proposes approximate annotations;
it does not move existing cuts or accept anything automatically. DP-10.3 now
meets its local scope. DP-10 remains Partial.

[Evidence and inventory](../../tools/model-qualification/evidence/2026-10-25-word-refinement/)
retain invocations, source and executed-binary digests, outputs, failures and
bounded continuations. Primary machine: Apple M5 Max, macOS 26.5.2, Rust 1.97.1.
The package was built from the recorded dirty working tree at 346bda75; final
changed code input hashes identify that code independently of the commit label.
This is local feature qualification, not a clean-machine or release-candidate pass.

## Implemented contract

The pure analysis layer owns bounded pronunciation-DAG alignment, unexplained
phonetic-context refusals and a frozen review against raw words, corrections,
selection and actual analysis extent. Starts floor and ends ceil to the existing
10 ms annotation clock. Raw model frames are 20 ms. Posterior/path support is
explicitly uncalibrated, not a probability that a boundary is correct.

The host prepares an exact-phase window from the verified Original, runs the
native CPU worker and verifies its contained output after process cleanup.
Initial audio cache opening still decodes the full stream. This does not
implement cursor-priority incremental range decoding.

The app captures immutable state, performs work off UI/audio/writer threads,
shows the measured waveform and current/proposed edges, offers comparison
audition and manual adjustment, then saves only explicit acceptance. A pinned
word, stale state, sleep interruption, unsupported language or neighbor crossing
refuses. One correction Undo restores prior words; authored history is unchanged.
The English-only pack refuses other cached language keys before model loading.

## Actual checks

| Scope | Result |
| --- | --- |
| Pure alignment/acceptance | Fourteen focused tests; all 24 observed reference structures preserved by the native port |
| Native features and emissions | Maximum absolute feature error 0.000355005; emission error 0.000228882; exact paths, intervals, offers and guard structures in all 24 observed cases |
| Native worker/control | Thirteen actual/native checks, including cancellation, EOF, malformed or second requests, unrelated cancellation, corrupt PCM/statistics, FIFO and symlink refusal |
| Store/headless | Final-guard rollback preserves value/version/Undo/Redo; stale, pinned, crossing and wrong-guard requests refuse; in-bounds manually removed words survive acceptance |
| Native UI | Final release visual replay: 29 checks plus shortcut audit; actual worker, cancellation, changed transcript, sleep, explicit Tab/Space acceptance, preserved pins and one Undo; 960x640 and 1280x820 controls visible |
| Device audition | Final release performance replay: 16 checks plus shortcut audit; real prepared PCM and reported device clock, pause/resume, exact natural endpoint, fresh restart and close; no starvation, device faults or padding |
| Packaging | Full ad-hoc hardened bundle: 77 Mach-O images audited, 285 crates in notices; general relocated/scrubbed bundle verification passed |
| Model/runtime | Original six-file offline pack imported and actually smoke-tested by the relocated packaged worker; no Python/Torch executes in refinement |
| Runtime integrity | 65 recorded negative-case operations: seven expected host refusals, valid re-signed substitutions, intact restoration, removed marker and unpinned host; both packaged hosts checked |
| Packaged acceptance | Real proposal, explicit acceptance and one Undo: correction versions 1 -> 2 -> 3, pinned neighbor and other words retained, complete authored document byte-equivalent as JSON |
| Developer/CI inputs | Pinned original ORT wheel treated only as data, exact library extraction and six header pins; 15 Python regressions, actual HTTPS preparation and same-inode lock recovery after SIGKILL |

The workflow fixture uses synthesized Samantha speech, not human timing truth:
Copper, birds, cross, quiet, roofs. Actual Whisper recognition supplies five
punctuated words. A manual neighbor pin holds roofs at 253..273 cs. Copper's
current 21..52 cs becomes the reviewed 16..68 cs proposal only after acceptance.
The document remains at revision f0081ed5-c0e6-452f-b125-dc5e7f612313.
The real comparison window is [0,68640) canonical 48 kHz sample frames.
Acoustic listening, physical input/display and VoiceOver speech are owner checks.

The complete final visual/device replay files are losslessly archived; compact
summaries and the [review](../../tools/model-qualification/evidence/2026-10-25-word-refinement/screenshots/review.png)
and [minimum-size view](../../tools/model-qualification/evidence/2026-10-25-word-refinement/screenshots/compact.png)
remain directly readable. The fixture archive is a verified portable project.

## Failures and continuations

The eight-package pre-commit run executed 3,590 tests: 3,587 passed, three failed
and 35 skipped. Two failures were the new test setup: an old three-pack
expectation and a native parity input directory still being produced. The latter
now has a distinct completed-observation input variable. The existing proxy
worker-kill test missed its observation window under the broad load; with state
diagnostics it passed alone in 0.949 s. The bounded three-test continuation passed.
The original broad invocation remains a failure, not a fresh full-suite pass.
Later English-language admission and clean-signing changes passed targeted
pre-commit checks; all 69 xtask tests and the 15 focused app tests passed.

Earlier compiler, lint, egui test-input and fixture interpretation failures are
retained. The first bundle failed because forced re-signing left nonzero old
signature bytes after the new superblob, despite successful system verification.
Removing the old signature first yielded two byte-identical clean reproductions.
Only private alignment/ORT copies are changed; Cargo output and qualified input
files remain unchanged. The strict parser and malicious trailing-data rejection
are unchanged. The first negative-case driver failed copying restricted system
file flags; data-only substitution fixed that driver without product changes.

## Quality and remaining scope

The stricter earlier experiment still fails: 65/126 positive offers and 65/91
retention (71.43%, below 80%); maximum foreign energy 1.4865% exceeds 0.1%.
Zero of 95 designated critical negative slots were offered, not zero of all
negative-context words. All 24 cases have been observed during development.
No unseen split, guaranteed precision or conversational/noisy-speech accuracy
is claimed. Read the [frozen experiment plan](../../tools/model-qualification/word-refinement/plan.md).

Specification 11.2 permits an on-demand approximate proposal that may improve
timing. These workflow checks do not relabel the experimental quality gates.
Human phonetic/listening judgment is recorded under specification 29.1 in the
[owner list](../REQUIREMENTS.md#to-verify-owner). Incremental scheduling, broader
sentence/pause/shot work and performance remain separately open.

Three older native workers still lack required compiled image/library and signer
admission before launch. DP-22 remains Partial; this alignment qualification does
not close that gap. Admission is also not containment against a concurrent
same-user writer who can replace the host itself.

Original checkpoint/statistics and runtime binaries are excluded from repository
evidence. The model pack records unspecified weight/statistics terms separately
from BSD source and CMUDict notices and grants no redistribution permission.
Accepted annotations require no model. See [word refinement](../WORD_REFINEMENT.md)
and [packaging](../PACKAGING.md).
