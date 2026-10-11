# YouTube import size bounds, 2026-10-11

The metadata and completed-file size checks now reject overflow and oversized
stream pairs before expensive work. Production download and Original limits
are unchanged.

## Failure and correction

Inspection previously summed two optional `u64` sizes without checked
arithmetic. A malicious or broken size pair could panic; the project also
enables overflow checks in release builds. An unknown size suppressed the entire estimate,
hiding an already oversized known stream. A fast download could finish between
watchdog polls and enter assembly without a final combined-length check.

The new checks use a checked sum bounded by the caller's limit. Known sizes
remain useful rejection evidence when the other stream is unknown; the total
estimate remains unknown in that case. Declared sizes take precedence over
approximate sizes. Confirmation checks the current acquisition limit again.
Completed files are checked before assembly and the checked total supplies
the assembly output budget. Overflow and over-limit pairs report
`YouTubeTooLarge`, create no project and clean up through the existing private
workspace ownership.

## Regression evidence

Both initial behavior tests failed against the old implementation: the
metadata test panicked on integer addition, and the completed-transfer test
reached the intentionally absent assembly worker with 1,100 bytes against a
1,024-byte limit. The passing versions verify refusal before confirmation or
assembly, no transfer where inspection refuses, and no project publication.
Additional cases cover an oversized known stream beside an unknown stream,
both positions, approximate sizes, the inclusive bound and a tighter limit
between inspection and confirmation.

The first test invocation lacked `DEADPAN_FFMPEG_PREFIX` and failed at the
native build scripts. The configured invocation records the actual behavior
failures. A later app-test command selected the empty library and ran zero
tests; the corrected command selects the app binary. Neither setup failure
is counted as validation.

Strict CLI Clippy and all 487 enabled CLI unit tests pass; four are skipped.
Two passing tests reported lingering output and both passed their isolated
follow-up without that warning. All five selected real-worker import/assembly
tests and fifteen app job tests pass. The app job test that requires an
explicit real URL returns early in this run; it is not new network evidence.

## Real packaged 4K import

The fresh 742.8 MiB ad hoc signed bundle passes `bundle-verify`, including
relocation into a scrubbed environment, its ordinary media smoke test, bundled
helper probes, AI runtime GPU smoke and helper/runtime/notice tamper negatives.
No AI model generation was repeated.

A second relocated copy runs `deadpan-app --headless` with a fresh home and
temporary directory, only the system `PATH`, and no inherited developer or
helper configuration. It imports
[CHARGE, Blender Studio](https://www.youtube.com/watch?v=UXqq0ZvbOnk), credited
as CC BY 4.0 in the [retained source reference](https://commons.wikimedia.org/wiki/File:Charge_-_Blender_Open_Movie-full_movie.webm).
YouTube's metadata does not supply a license value; the app preserves that
absence and shows its rights notice.

The ordinary selector chooses 3840×1608, 24 fps SDR AV1 format 401
(111,440,722 bytes) and 48 kHz Opus format 251 (4,365,234 bytes). The complete
import takes 37.439 seconds on the M5 Max/128 GiB, macOS 26.5.2. Reopening,
validation, document inspection and project diagnostics pass. The project
retains the 3840×1608/24 presentation basis and one full-source baseline of
6,306 edit frames. No preview-sized transcode is used. Signature verification
passes afterward, and no private download workspace or creation staging
package remains. This is import timing with uncontrolled OS caches, power
and thermal state, not a playback or Render benchmark or second-Mac proof.

The executed packaged app SHA-256 is
`79c5cfa36ffac7f1b3c4aab8a9805fc01441e914573a91355ed1d333f15ec93c`.
The first harness stopped after the successful helper probe because it tried
to parse one line of pretty-printed JSON as a complete object. The corrected
parser reused that recorded success and continued; no import had started.

## Scope

This is a shared import-boundary change with no UI, decoder or native ABI
change. No full workspace gate, sanitizer suite or visual replay was repeated.
The previous thumbnail qualification retains the relevant UI checks. DP-14
remains partial while the remaining import acceptance and security audit is
completed.

[Evidence](../../tools/media-qualification/evidence/2026-10-11-youtube-limits/README.md)
retains the failing and passing test commands, setup failures, sole-agent diff
review, bundle provenance, actual import reports and source/executable hashes.
The app and imported project remain under
`/tmp/deadpan-youtube-limits-20261011/packaged-4k`.
