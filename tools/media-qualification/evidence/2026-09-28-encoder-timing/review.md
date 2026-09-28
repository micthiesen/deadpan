# Encoder experiment review

Parent owns all compiler, unit, media, sanitizer and Git execution serially.
Audio worker owns pure Python oracle/tests; PCM worker owns C probe/helper
hooks. Parent owns orchestration, MP4 inspector integration and evidence.
Independent read-only review covers parser, runner and C/oracle correctness.

Before execution, review corrected library symlink rechecks, before/after source
attribution, whole-experiment process fault handling, and a test that assumed a
changed library was the first admission row. Fifty-two tests then passed.

Initial native matrix failed video checks because unspecified decoded hardware
SAR was not resolved from the explicit container value and raw H264 hashes were
incorrectly required to survive Annex-B-to-MP4 framing changes. Those are
observer corrections, not changes to encoded timing or media. The original
matrix remains failed. Original code bytes are retained with matching hashes.

Independent oracle review found ignored AV_FRAME_FLAG_CORRUPT. A follow-up
initially confused numerical bit values; parent checked pinned frame.h before
execution and set CORRUPT=1, KEY=2, adding a valid-keyflag regression. SAR fallback
uses only explicit positive stream evidence for unspecified raw SAR; malformed
evidence cannot bypass other identity/PTS checks. Final independent review
reported no further finding.

Final 58-unit pass, video-only re-observation of original files, original
40-assertion compatible fixture, and 13-case ASan/UBSan matrix completed. No
process/instrumentation fault or source/admission drift occurred. The selected
no-edit-list path still fails 60 fps timing by +1024 AAC samples. Both real
matrix invocations exit 1. Source, decoder, GOP and consumer limits remain
explicit; no fallback, tolerance increase or spec amendment was made.
