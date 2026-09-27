# Sound catalog audition and exact route preparation

This increment adds audio-only catalog audition to the native workspace. It uses
the shared canonical playback service and measured source endpoint, with separate
sound selection and sample cursor. Sources teaches `j/k`, Space play/pause/resume
and Shift+Space whole-sound looping beside the selected sound. Leaving the pane
or changing sound revokes playback and resume. Text, composition, focused native
buttons and help retain input ownership. Sound delivery requests no picture and
does not move Original/edit cursors, selected beat or group. A legitimate editor
picture request already in flight may finish normally.

The backend admits a qualified audio-only `Sound` target against its captured
asset, receipt, original and project rate. A temporary Source view is never
written to history. The measured 48 kHz boundary excludes whole-video-frame
enclosure slack, including at loop seams. Unknown source priming stays present.
Audition uses the existing source preparation, limiter, monitor gain and device
generation path. Sound placement, voice treatments, scoped Hold allowances,
final mixing and export remain required. Core 28 and database 34 are unchanged;
no DP requirement or gate is promoted.

`SoundRoute` separately supplies bounded exact recipe windows and monotone
Keep/Gap/Sequence/Repeat ripple maps. It retains source phase through deletion,
insertion and fractional selections, with indexed bounded queries over compact
repeats. This pure kernel is not installed in authored sound events and does not
define a sampled lattice or PCM processing history. The
[integration contract](../SOUND_EVENTS.md) retains the complete remaining work.

The CLI now keeps an LRU of qualified private PCM with 16-entry, 1 GiB physical
PCM and 1,000,000 indexed-audio-frame caps. Cold verification precedes eviction;
both reservations publish only after successful preparation. Hot hits recheck
captured asset, compact receipt identity and original/source hashes. Full receipt
snapshots are no longer duplicated in the cache.

Independent general, keyboard/state and resource reviews covered the increment
against its saved pre-change working tree. A real nested serde failure was fixed
by validated wire deserialization and regression coverage through JSON values and
a containing tagged enum. The resource review found duplicate receipt/index
residency outside the PCM budget; compact identities, aggregate metadata eviction
and an exact decoder-index cap address it. The follow-up reviews found no remaining
actionable issues in those scopes. A proposed cancellation of pre-existing editor
picture work was withdrawn because it would strand the intended editor preview.

Initial test failures are retained in the evidence. Plain WAVs lack declared
speaker layouts and correctly fail production PCM qualification; audible cache
tests now use the existing qualified AAC fixtures. The real WAV fixtures still
test measured 44.1/48 kHz descriptor clocks without claiming playable layout.
Two backend test setup failures were corrected: a parent-traversing fixture path
is canonicalized, and the cache-switch witness now includes both known fixture
impulses instead of testing their near-silent decay. No PCM assertion or production
media contract was weakened. Test-only LRU identities append admitted empty MP4
`free` atoms without changing their audio.

The built-in `image_gen.imagegen` tool produced the
[sound audition board](../design/boards/sound-audition-board-v1.png) from the
existing workspace reference. Its [exact prompt](../design/prompts/sound-audition-board-v1.txt),
dimensions and hashes remain in the repository. The board specifies hierarchy,
separate clocks, focus, selection, ordinary keycaps and ready/playing/paused/loop
states. It is a visual target, not a screenshot or acceptance result.

The contributed production UI harness is preserved and extended with
`sound-playback`. It uses real sound registration and production input/widgets,
then explicitly simulated delivery for playback transitions and stale/fault
coverage. Actual AAC backend tests supply separate PCM evidence; simulated UI
delivery does not establish audible output or device timing.

Focused verification passed 11 exact route tests, six Sound backend tests, seven
final cache tests, 17 existing CLI audio inspection/context tests and 236
app/harness tests. These counts describe separate invocations and overlap the
workspace suite; they are not additive coverage totals. The final selected-name
label was added during workspace lint and independently reviewed. The later
workspace tests/build and feature lint/tests cover that label, followed by a
separate formatting check and final source seal.

The full workspace test invocation finished with 1,743 passed, four failed and
none ignored. The artifact file-type test could not create its Unix socket in
this sandbox (`PermissionDenied`, at `artifact.rs:200`). Three existing playback
tests hit the worker-wait deadline at `tests.rs:256`: canonical source delivery,
Original leading/trailing audio timing, and Original/edit cache separation. All
six new Sound tests passed in that run. The complete failed invocation is retained;
it is not a passing workspace gate.

Each of those three playback cases subsequently passed in isolation with the
same test binary, whose before/after SHA-256 is retained. No source, timeout or
assertion changed for the retries. The parallel failure mechanism remains
unestablished. Formatting, strict workspace Clippy, workspace build, `doctor`,
strict app/harness Clippy and all 236 final app/harness tests passed.

Visual replay executed the final debug binary and passed the live Kestrel audit:
3,472 routing cases against 62 reserved bindings, with no conflicts or source
drift. `sound-playback` then stopped at Metal initialization with `No adapter
found`, before app construction. It ran zero sound scenario steps/assertions,
captured no images and measured no timings. The saved board remains an inspected
design target; the coded layout's aesthetic match and actual painted interaction
are unverified. Native physical keys, IME, accessibility and acoustic output were
not exercised by this increment.

The separate optimized build succeeded and the release replay passed the same
live shortcut audit, then failed at the same Metal adapter boundary. It also ran
zero sound scenario steps and recorded zero timing samples. Build time is not
application latency. Both replay reports retain their distinct binary identities.
The final formatting check passed with all 572 source/config hashes unchanged.

Verification results and runtime limits are recorded in the
[retained evidence](../../tools/media-qualification/evidence/2026-09-27-sound-audition/README.md).
Git metadata is read-only in this session, so no commit or push is claimed.
The full-project goal remains active.
