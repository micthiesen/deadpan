# Original and selection audition qualification

This increment adds read-only Original playback and selection loops. It does not
complete a requirement or delivery gate. Core schema 27 and database schema 33
remain unchanged.

Space plays or pauses the current Original or edit; Shift+Space loops the selected
Original moment or edited beat. Context defaults to 500 ms before and 750 ms after,
bounded by the full captured domain. The command
`:audition-context lead=0ms follow=0ms` requests an exact selection. Visible
controls and contextual help teach both shortcuts. Paused loops explicitly show
**Resume loop**. Navigation discards resume state; faults cannot restart playback.

The project service prepares the Original descriptor from the receipt's full
measured A/V union, keyed by receipt, asset and project rate. It shares the picture
index with the workspace. Original playback preserves audio before and after the
picture, uses retained PTS for picture positions, and never changes the edit's
cursor, selected beat, viewed group or history. Canonical PCM preparation keeps
the authored snapshot for source admission and compiles a separate validated
Source-only view. Original and Sequence plans have separate cache identities.

A loop captures one immutable half-open sample window. Device delivery increases
monotonically across laps; content position wraps inside that window. Bounded
reads retain the complete canonical DSP plan and its history. Tiny loops reuse a
verified lap within one bounded batch. Resume retains exact delivery samples,
including positions after multiple laps. Picture scheduling admits one generation
and one active decode/GPU request; a bounded nonloop's terminal picture comes from
the last included sample even when its cursor names excluded Out.

Three independent reviews covered general correctness, clocks/PCM/cache admission,
and keyboard/focus/harness behavior. General review found that zero-context loops
at the Original's first/final picture incorrectly used full A/V union endpoints.
The fix adds separate measured selection endpoints. Whole-Original playback still
includes audio lead/tail; exact selections exclude it until explicit context adds
it. Real offset and VFR fixture regressions exercise both policies, large clamped
context and invalid negative context. The reviewer confirmed the correction. The
other two reviews reported no findings. These were static reviews, not listening
or native visual qualification.

The contributed UI harness is preserved and extended with `original-playback`.
It drives production Space/Shift+Space and pointer controls, changes context,
checks exact half-open loops, multiple laps, pause/resume, stale request/generation
rejection, navigation stop and visible faults. It verifies that Original playback
preserves the edit context and project revision. Its device updates are explicitly
injected; independent playback tests exercise actual canonical PCM through a
controlled device queue. Neither kind of test establishes acoustic synchronization.

The loop PCM regression delivers 18,432 samples per generation through 256-frame
callbacks, including an offset resumed generation. This crosses two 8,192-frame
preparation boundaries and many 17-sample loop seams while checking every sample
against the canonical selected output. Other backend tests cover audible audio
beyond picture EOS, an explicit 24 fps project with different source cadence,
Original/Sequence cache isolation, immutable history, bounded EOS, cancellation,
lost reports and checked delivery overflow.

Initial failures are retained: a core test fixture used an invalid content hash;
a tail-audio assertion sampled silence between the fixture's deliberately sparse
impulses; and the first app run passed fixture paths containing parent traversal
to the normal original admission API. The corrected tests use a valid hash,
measure the actual last impulse after picture EOS, and canonicalize fixture paths.
No admission rule or test was disabled. The first formatting check also requested
one normal rustfmt line wrap.

Verification results and the final source identity are recorded in the retained
[evidence](../../tools/media-qualification/evidence/2026-09-27-original-audition/README.md).
The existing ten ImageGen boards and exact prompts remain the interface targets.

The final source passes formatting, strict workspace Clippy, the workspace build,
the CLI doctor, and strict app Clippy with `ui-harness`. Workspace tests report
1,682 passed, one failed and zero ignored. The sole failure remains
`directories_fifos_and_sockets_are_rejected_without_blocking`: this sandbox denies
the test's Unix socket bind with OS error 1 (`Operation not permitted`). App tests
with the harness feature report 226 passed, zero failed and zero ignored. These
test groups overlap and must not be added together as unique tests.

The final GUI invocation passes all 3,472 production key-routing cases against
62 live Kestrel reservations, with no source drift or conflicts. The
`original-playback` scenario then fails before constructing its application:
Metal reports `CustomNativeAdapterSelectionError("No adapter found")`. It executes
no scenario steps and produces no screenshots. A native app lookup by the name
`Deadpan` also returned `Invalid app: Deadpan`; no alternate live surface was
established. The release timing run was not repeated after this initialization
failure. Rendered layout, visual comparison with the ImageGen targets, live
native focus/IME and physical audio remain unverified for this increment.

All 549 source/configuration paths match the seal captured before workspace tests
began. The 24 reviewed source paths also match the final tree. Formatting and
workspace Clippy were repeated after the last review correction. The evidence
retains initial failures as well as final results. No requirement or gate was
promoted, and the full-project goal remains active.

Git metadata is read-only in this session, so no commit or push was possible.
The pending changes include the contributed harness and prior project work.
A verified patch and untracked-file archive are retained under
`/tmp/deadpan-original-audition-20260927/checkpoint`; they preserve work but do not
replace the requested Git commit.
