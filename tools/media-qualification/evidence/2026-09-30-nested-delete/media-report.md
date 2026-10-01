# Nested Partition DeleteRange: independent media/plan verification

## Scope and state

Added four PCM tests and two picture-plan tests. Root owns all Cargo, decoder,
process and native execution. Root's corrected focused runs pass all six tests,
including the final unframed picture variants. This agent ran only scoped
Rust 1.97.1 rustfmt and diff whitespace checks, both clean.
No production changes, commits or GUI operations were made by this agent.

Changed files:

- `crates/deadpan-audio/tests/composite_insert/delete_range/nested_partition.rs`
- `crates/deadpan-audio/tests/composite_insert/delete_range.rs`: child declaration only
- `crates/deadpan-plan/tests/edited_slice/delete_range.rs`
- `crates/deadpan-plan/tests/edited_slice.rs`: child declaration only

Both declarations were coordinated with root. Existing qualified PCM, picture,
edit/inverse and cold/reverse-read helpers are reused. Core's public command and
preflight APIs remain unchanged. The old admission rejects wholly untreated
nested chains: the unframed picture variants and untreated terminal/root-sound
PCM cases exercise that change. Treated chains were already admitted and provide
retained-context regressions. An initial comment overstated the old rejection;
root's static review corrected it during the first focused runner. The unframed
picture variants were added at that point and need a fresh run after source froze.

## Picture-plan oracles

`two_and_three_nested_windows_keep_every_source_or_freeze_point_and_framing_clock`
uses both two and three unity windows, with and without owner framing, to expose
exactly leaf `[3,10)`. Delete
`[1,3)` must retain old global frames `[0,3,4,5,6]`, corresponding to Source
ordinals `[3,6,7,8,9]`; Freeze selects ordinal 5 throughout. Each output frame
checks exact provider ticks, original indexed ordinal and PTS, retained captured
geometry, leaf/intermediate owner positions and complete owner durations.
Independent documented Q32 interpolation verifies every framed owner pose.
The live root uses the shortened five-frame clock and applies once.

`endpoints_in_different_nested_children_preserve_context_live_ancestors_and_mark_bias`
deletes global `[4,12)` inside a framed ordinary Sequence containing a two-window
Source and three-window Freeze. The expected complete old-frame order is
`[0,1,2,3,12,13,14,15,16,17]`; expected source ordinals are
`[0,1,3,4,5,5,5,5,20,21]`. It checks each indexed PTS, captured context, each
retained framed owner, and the live six-frame parent/ten-frame root clocks.
Root marks preserve keys and bias: prefix1 stays1, start4/Left and end12/Right
join at4, suffix17 becomes9, and an interior mark becomes unresolved. Both
tests apply the exact inverse and require the complete original document.

## PCM oracles

PCM uses the existing SHA-256-verified `pcm-mono-44100.wav` fixture, measured
audio index and explicit mono-to-stereo provider. Only the qualified decoder and
reconstruction kernel are shared with production; expected coordinates do not
come from RenderPlan, a binding or preflight output. Every comparison uses the
existing 2e-6 independent reconstruction tolerance; preservation comparisons
between saved and moved samples are exact f32 equality.

1. **Two/three windows, Source/RoomTone and gain.** Both window chains expose
   leaf `[3,10)`, and `[1,3)` is deleted. With `T=8008/5` mix points/frame,
   prefix phase is `3T=24024/5`; suffix phase is `B(3)+3T=48049/5`.
   Preserve `[0,1602)` and `[4805,11211)`, remapping the latter to `[1602,8008)`.
   Source expectations use the original 44.1 kHz `147/160` sampling ratio;
   RoomTone uses a hand-composed loop/crossfade reference with 19220 complete
   host points. Each retained sample is checked cold at the end, in reversed
   irregular reads, and again warm. Interior authored bus windows independently
   check leaf+6/intermediate-3/live-root-6 dB, applied once.

2. **Different partial children and continuing support.** A two-window Source
   followed by a three-window RoomTone loses `[2,10)`. The Source prefix has
   3203 samples. The RoomTone suffix starts at old global B10=16016 with local
   phase `B10-4T=48048/5`. Old `[10,14)` has 6406 samples, while new `[2,6)`
   allocates 6407. The last point at leaf16015.6 is independently reconstructed
   and must be nonzero because the full twelve-frame physical owner continues.
   All existing suffix samples match exactly, with cold/reverse/warm reads.

3. **Exhausted terminal support.** Three `[0,4)` windows around Source4 lose
   `[2,3)`. Old suffix `[B3,B4)` contains 1601 samples; destination `[B2,B3)`
   contains 1602. The original physical source endpoint is independently
   `ceil(100+4*1471.47)=5986`. The last old supported sample is nonzero and
   preserved; the extra point mapping to old sample6406 is exactly silent and
   reported as suppressed. This distinguishes retained discrete root support
   `[0,6406)` from continuous extent6406.4.

4. **Root sound route transformed once per command.** A picture-only Source
   under three windows supplies time without introducing a Hold silence gate.
   A qualified root SoundEvent retains its exact recipe. Two deletions,
   `[0,1)` followed by `[2,3)`, produce exactly two matching root route edits.
   Independent chronological sample labels are `B1+n` before the second join
   and `B1+B3+(n-B2)` after it. Authored bus reads at100,3303,3903 compare with
   original sample phases1702,6507,7107 in shuffled order on one warm reader.
   Windows avoid new edit fades; they do not treat a root sound as copied voice.

## Focused commands for root

```sh
rustup run 1.97.1 cargo test -p deadpan-plan --test edited_slice --locked delete_range -- --nocapture
rustup run 1.97.1 cargo test -p deadpan-audio --test composite_insert --locked delete_range::nested_partition -- --nocapture
```

## Limits

Picture evidence is synthetic canonical planning, not decoded/GPU pixel proof.
PCM evidence is qualified reconstruction and authored gain/root-bus behavior,
not device delivery, acoustics, mastering or export. Source and RoomTone cover
audio; ordinary Freeze and captured geometry are covered in the plan tests.
Static gain factors establish treatment application, while exact changing
owner-clock interpolation is checked through picture framing. Unsupported
Repeat/non-unity Preserve/Generated endpoints, pools, depth/overflow and general
command atomicity are owned by the core suite. Native persistence is root-owned.

## Root-run results

The initial focused plan run passed two tests, but preceded the final unframed
variants and therefore does not prove those variants. The first audio run passed
the terminal-support and existing root-sound-route cases. Two other tests failed
at `composite_insert.rs:372` because their independent RoomTone oracle asked the
reference resampler for more than its 256-frame limit (`InvalidRecipe: block must
contain 1..256 frames`). The new `room_samples` test helper batches that oracle
   at 256 points, deriving each chunk phase from the original absolute start plus
the integer offset. No coordinate, expected support or production code changed.
The initial log is `/tmp/deadpan-nested-delete-20260930/audio-delete-final.log`.

Corrected audio: **4 passed**, none failed/ignored; 16.7909 seconds including
build, 5.75 seconds reported test runtime. Final plan, including unframed variants:
**2 passed**, none failed/ignored; 10.9560 seconds including build. Retained
logs/metadata are `audio-delete-corrected.{log,json}` and
`plan-delete-unframed.{log,json}`. Both runs use source manifest
`6fe99501eec223fc725bb1edd7844b80c23e15c5834e680a4430b3fbe38d53cb`
over base `344ec32f243d8f5ccc45cecc7596a089ce0d827f`.
