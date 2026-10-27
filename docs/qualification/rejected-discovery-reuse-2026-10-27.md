# Rejected current-occurrence discovery reuse, 2026-10-27

The call-local discovery memo is removed. It avoided building the same current
occurrence batch twice, but the unchanged real-WAV Split matrix remains four
passes and three work-limit failures. No requirement, criterion or gate closes.

The prototype retained exact live plan/context/range batches under the existing
residency ceiling. Optional key and heap scans used the existing bounded cache
ledger; required construction and DSP kept their canonical charges. A focused
witness proves an identical second construction disappears without changing PCM,
dependencies, depth or provider calls. Neither work nor PCM limits increased.

Review found that retained metadata can starve later required PCM. A retry
released the memo and rebuilt the complete graph with reuse disabled, preserving
all actual work charges. A later caller audit found that a nested
`TailInput::Sends` read can create a new graph with reuse enabled during that
retry. A controller-scoped disable guard would be needed to retain this design.
It was not added because the matrix showed no acceptance improvement.

## Evidence

Apple M5 Max, macOS 26.5.2, Rust 1.97.1, locked dependencies and qualified
FFmpeg. Audio tests use optimization level 2 with debug/overflow checks.

| Check | Result |
| --- | --- |
| Focused discovery and tight-residency tests | Eight pass, zero ignored. |
| Current Audio library | 202 pass, zero ignored. |
| Strict all-target Core/Plan/Audio/Store lint | Pass. |
| Full real-WAV Split matrix | Four pass, three fail in 367.54 seconds. No calibrated speed claim. |
| Restored source check | All 1,387 preceding qualification inputs match exactly; six files restored and two prototype files removed. |
| Restored strict check | Pass with stable source inputs. |

The incomplete comparisons remain all-owner Source/Sequence `cut-three`, limited
`0+511`, and selected/nested selected `prefix-two`, limited `0+4096`. They fail
on plan/construction work bounds. Full authored/faded/limited processed root-sound
refinement remains passing.

Three earlier fixture failures are preserved: an invalid unscoped range, a
silent Hold in the intended audible witness, and an omitted enclosing Preserve
scope in a saved journal. Repairs change only those fixtures and preserve the
production guards. The final candidate executable has SHA-256
`43dd24260336fe33f66c88eda2a62398a60a91611db390d1ff51637b14766d53`.

The [evidence inventory](../../tools/media-qualification/evidence/2026-10-27-rejected-discovery-reuse/inventory.json)
retains commands, failures, source/binary identities, the exact rejected overlay,
review findings and restoration proof. All 42 compressed payloads verify their
stored and expanded hashes: 1,465,178 stored bytes and 5,823,594 expanded bytes.
Inventory SHA-256:
`48f0793dc40e68d152b52f1b64cc2f3ace9cb70e2b7fb34309134864caa34278`.
Raw records and the executable remain in
`/tmp/deadpan-discovery-reuse-2026-10-26`.

The next investigation factors exact scalar occurrence support out of the full
constructor. Clock discovery needs ordered stable paths and exact processed
sample bounds, including Preserve history hidden by exterior crops. It must
retain recipe validation, scope checks and actual work charges. Full playback
construction and media admission remain required before PCM. Geometric owner
visibility or a source's audible selection cannot substitute for processed
allocation. This proposed replacement is not qualification evidence.
