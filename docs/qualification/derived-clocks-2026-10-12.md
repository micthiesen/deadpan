# Persisted effective Retime clocks, 2026-10-12

Core 54 stores `derived_clocks`, keyed by the owning Retime. Each record contains
an exact output extent and a half-open exact child window. Integral Source,
Hold and Retime recipes remain separate. Sequence and compact Repeat allocations
sum their effective children without rounding; the final project extent must
be integral and every extent must remain within the existing frame bound.
Admission checks ownership, child bounds, positive extents, representable slopes
and unity timing for transparent Partitions. Hanging-tail admission uses the
effective speed, including when the retained recipe itself is unity.

`ProjectDocument::exact_durations` returns the effective graph. Root and
single-node integral queries return exact integral values or refuse a fraction.
Legacy whole-document duration maps refuse derived clocks, including inside
cached validation scopes. `recipe_durations_shared` explicitly supplies the
separate intrinsic graph to the plan compiler. Both graphs are validated.

Production plans use the persisted Retime maps, exact Sequence/Repeat prefixes
and effective root duration. They retain the original recipe counts for
intrinsic processing. Documents without derived clocks keep the existing
integer Repeat storage path.

Reversible patches carry clock changes, guarded before-values, inverse values,
changed owner IDs and actual project-duration deltas. Snapshot restore retains
the whole composition. SQLite schema 82 closes the new wire contract; older
unused development packages are refused under the owner's development-format
authorization, without a migration or compatibility shim.

## Verification

Base `37b7c3a6`; locked dependencies and Rust 1.97.1 on the same Apple M5 Max
machine. The primary implemented and self-reviewed alone. No live GUI or
physical audio output was used.

Core tests cover JSON persistence, cached query behavior, clock-only patch
round-trips, stale patch refusal, actual restore deltas in both directions,
legacy edit refusal, malformed and orphan clocks, unsupported Preserve,
duplicate keys, obsolete schema, nested bounds, unity Partitions, hanging tails,
arithmetic exhaustion and a compact billion-play fractional Repeat.

Production plan tests now load the former injected fractional fixture from
persisted document JSON. They verify exact source coordinates, normalized
framing, root RoundEven allocation, definition PointCeil grids and owner windows.
An additional persisted Repeat case has an intrinsic four-frame recipe graph
and an actual three-frame project; it checks stable play IDs and absolute sample
boundaries `0, 801, 1602, 2402, 4805` at 30000/1001 fps.

The SQLite test creates and reopens a derived-clock package, proves a refused
legacy edit leaves the snapshot intact, saves/restores a named take, reopens,
undoes/redoes the restore and reopens read-only. The core patch test separately
proves changes to the clock table itself and nonzero duration deltas.

The decoded-WAV test reads the retained `pcm-stereo-48000.wav` fixture through
the real source decoder. Two Retime extents of `1/2` and `3/2` frames allocate
root samples `0..801` and `801..3203`. Independently written resampling recipes
require slopes `4` and `4/3`; the second allocation retains source phase `4/15`
at sample 801. The oracle shares the qualified resampling kernel, not the plan's
timing calculations. Every sample agrees across 1-, 31- and 256-sample reads,
including a direct seam read. This proves raw source PCM before effects, not a
mastered or encoded export.

Three initial PCM fixture runs exceeded explicit limits: the reference sampler's
256-sample output block, the production reader's same output bound, and the
fixture decoder's input-read bound at four-times speed. The test now uses bounded
output chunks and a 4,096-sample decoder bound for rate/filter support. No
production limit was relaxed. Those failed runs are retained with final evidence.

The first broad build found two existing `command_work` test callers that still
treated the cached duration-map API as infallible; those integral fixtures now
unwrap its result explicitly. The final core, plan, audio, playback and store
run passes all 2,927 tests with none failed or ignored. Those sources stayed
unchanged while the separate native admission check was added afterward.

Self-review found that the native snapshot builder could admit a clocked plan
before its controls support exact editing. It now refuses such a workspace with
an explicit error, before exposing integer editing controls. A headless service
test verifies refusal without authored changes; no window or device is opened.

That native test passes. Strict all-target Clippy passes for core, plan, audio,
playback, store, CLI and app with warnings denied; workspace formatting passes.
The [evidence](../../tools/media-qualification/evidence/2026-10-12-derived-clocks/metadata.json)
retains complete logs including failed setup runs, final source and executable
hashes, the decoded fixture hash, toolchain/hardware identity and self-review
scope. No backend regression source changed after its passing run.

## Remaining integration

This is persisted-clock and compilation support. No ordinary edit constructs
these clocks yet. Ordinary reducers refuse derived-clock documents; admission
also refuses marks, sound events, beat sounds, sound routes/allowances, retained
audio lineage/bindings and all Preserve/Shift Retimes when clocks are present.
The native workspace also refuses these documents until exact editing is
integrated. These are explicit temporary boundaries, not reduced requirements.

Owned Preserve input/output routes, atomic insertion, exact mark/sound lifecycle,
copy/isolation/split/delete/trim/move/setter transforms, native editing and scoped
generation admission remain required. Retime insertion stays guarded. There is
no new full-gate, performance, live-native, physical-audio or encoded-media
qualification in this increment. No requirement or gate is newly complete.
