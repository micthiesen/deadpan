# Partial-copy mastering context, 2026-10-12

The mixed Original/attached-sound comparison left open by the
[partial-copy qualification](partial-sound-copies-2026-10-12.md) is explained by
current master-limiter context. The copied authored mix preserves its PCM.
The limited mix has different gain because a louder Original transient now
precedes it. No production processing change is needed for this witness.

The new regression creates a real one-Original project through the CLI,
registers stereo PCM, adds an attached sound with -12 dB gain and nonneutral
EQ, and applies the same nonneutral EQ to its Source owner. Original audio
remains enabled. It copies frames 2..6 to the end of the 120-frame Original
at 30000/1001 fps.

The comparison reads 256 stereo frames at samples 3715 and 192704 through
`inspect-audio --authored-bus` and `--limited`. Every authored sample's f32 bits match
across the copy. Master gain is 1.0 before the copy and reaches
0.9466336984187365 at its new placement. The preceding Original window,
191936..192192, peaks at 0.947795033454895 after EQ. The maximum limited-output
difference is 0.018082082271575928.

For both placements, the regression independently reads the authored bus and
checks that each limited output sample equals that input multiplied by its
reported linked master gain, rounded to f32. The gain prediction error is zero.
This distinguishes retained content from current mastering without weakening
the copy comparison or requiring mastered output to ignore its new surroundings.

The initial diagnostic also checked the Original alone, dry and with EQ. Its
authored differences were zero and approximately 3.5e-18 respectively. Adding
the attached sound made the difference visible after mastering while its
authored mix still matched exactly. Those diagnostic logs are retained.

The final regression passes in debug and release with identical numerical
results. Formatting and strict CLI/all-target Clippy pass. The existing
partial-copy release export passes again: seven pictures, 19 audio blocks and
zero sample offset. The first lint invocation incorrectly requested the app-only
`ui-harness` feature from the CLI package; the corrected command and failed
invocation are both retained. Production code is unchanged from the preceding
qualified increment, so its broader suites and app build were not repeated.
[Evidence](../../tools/audio-qualification/evidence/2026-10-12-copy-mastering-context/README.md)
includes the fixture project, numerical reports, export rerun, diagnostic logs,
source/binary hashes, test patch and self-review.

The primary agent performed the investigation and self-review alone. No live
GUI, device output or listening check ran. This closes the specific mixed-bus
uncertainty from the partial-copy record, not the remaining attachment transforms,
mastering qualification or any DP requirement or release gate.
