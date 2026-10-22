# Fractional retained processed-owner support, 2026-10-21

Scratch draft only. Root owns final qualification, documentation and collection.
Base: `25646268`. Final source/binary identities: ROOT to record after all checks.

The change broadens one retained-overlap probe predicate in `audio_owners.rs`
from the immediately preceding point to any preceding point whose exact
transferred interval strictly crosses a positive allocation's start. The exact
sample coordinate, negative owner clock and rate stay unchanged. No DSP,
schema, encoder, UI or native timing-gate change is claimed.

Old-code evidence has two separate valid PCM regressions. The complete
nonlinear-bus witness failed with `Unsupported("nonzero Original outside
meaningful owner-clock support")`; binary SHA-256
`a660e05818178706644d3413387a934e13209ad6486367965f469353ca1ee8c2`,
witness SHA-256 `c390fdaaf50962dea7966634a884176b2d9fff2af1f71ed9d08af44bdb134d6e`.
The child-gain/envelope witness separately failed with
`Unsupported("nonzero Original outside owner support")` in the shared-EQ path;
binary `2df57c18ac8177b58b25b40014f9d6cf29dc337a5cdf2307bd07af40efd649e5`,
witness `68c3b8ab2d39d4a283927234885a479425722ddaa25de13824831ba5840b4b3d`.
Both ran one test with zero passes and exit100 against the old predicate.

The first plan attempt did not compile: ambiguous sample-grid `boundary` and
`probe` methods, E0034, exit101. After explicit grid typing, the old predicate
ran four tests: two valid positive ownership failures, one strict negative
pass, and one invalid factory setup failure. That fourth fixture cleared a
Source's video and audio, causing `SourceRangeInvalid` before the ownership
query. It is not a third valid positive failure. With the new predicate the
first plan run had three passes and the same setup failure. Replacing blank
Source time with an ordinary Hold preserved the geometry and made the fixture
valid. The corrected focused plan run passed all four tests in 0.011s.

The eight focused PCM tests passed in 3.525s. Executed binary SHA-256
`7f42acd547948a4d49907d4cefb44970358c1af641cd70605537737a00b76821`;
source `11e34ed89b079fe41b95c1a72fefca59bae7c87f86e115c0e9b6beade6b8fa42`.
They include the complete bus, child gain/envelope with exact negative Hold
clock, preserved independent-event phase, irregular/cold reads and the strict
genuine exhaustion at sample 12812. Three fresh static reviews reported no
findings. Review outcomes and actual commands remain in the final evidence.

The first release fixture compilation failed E0618: local `original` PathBuf
shadowed the `original(...)` expected-picture helper. Root retained the failed
source and log and renamed only that local to `original_path`; no production
change or encoded runtime pass is implied by this compile correction.

The first release runtime also failed before target behavior, during Original
admission. QuickTime MOV wrote nclc length 10 for H.264, where the strict
grammar requires nclx length 11; its sowt PCM entry was independently outside
the admitted audio grammar. The corrected producer uses ISO MP4 and AAC 192k
source audio with explicit delay/padding edits. A pinned existing release CLI
admitted that source with exactly 192192 presented audio samples and all 120
unchanged video packets. This is only a source admission witness; the full
release test remains PENDING. Details and failed atoms are retained in
`producer-admission/diagnosis.md` and adjacent metadata.

Encoded qualification is PENDING. The new fixture creates deterministic,
nonperiodic stereo 48kHz PCM with asymmetric small amplitudes and DC offsets,
encodes its audio to AAC 192k and stream-copies the measured CFR picture stream
into ISO MP4 with explicit 48000 movie clock and edit lists.
The actual A/V Original is registered and qualified through the public project
path. Both Source and RoomTone read that same real nonzero Original. This is
not the previous milestone's quiet Original fixture. The generator recipe,
producer executable hash, generated WAV/MP4 hashes and measured source
identities are retained only after execution.

The authored fixture puts +6dB saturation on fixed Preserve output and +3dB
saturation on root before a genuine lead Hold 3→4 journal. An independent
44.1kHz event has −18dB trim, +37 mix-sample offset, selected native [3100,8005),
retained phase 2480147/800 and mix support [6443,11782). The interior RoomTone
Hold then grows 2→3 while the processor remains four frames with input [1,8).
No Original muting, asset substitution or event alignment is used.

Before and after, a public portable copy retains the same authored history;
public commands remove only the event and the two nonlinear treatments to
obtain current physical PCM. Direct qualified event resampling and literal
processor/root equations independently predict all 17618 authored samples.
Both current physical and mastered PCM must be nonzero at fractional seam 8236;
the exact complete suppression mask contains only exhausted point 12812.
Controls distinguish stale physical input, separate-voice saturation and
omitted processor/root stages. These controls must pass before export.

The pending release test requires all 11 pictures with literal provenance
`Background×4, Original13, Background×2, Original13, Original12, Original13,
Original14`, exact Original PTS, and no picture anomalies. Two contiguous
signal windows [0,8809) and [8809,17618) cover every presented audio sample.
Each must pass existing fidelity limits with `verified_zero`, measured offset 0,
no low-SNR blocks and summary.nonzero_offsets 0. Presented end and edit-list
media ticks must both equal 17618. No tolerance change is allowed.

ROOT actual final results: release encoded PENDING; broad plan/audio/playback
PENDING; strict lint PENDING. Do not convert these obligations into pass claims.
The collector requires root's finished `runtime-results.json`, actual log
counts, executed binary hashes, final source hashes and `--confirmed-final`.
It omits raw media/binaries, compresses reports/source snapshots and rehashes a
bounded inventory under 8MiB. No GUI interaction or native Retime timing-gate
qualification is claimed.
