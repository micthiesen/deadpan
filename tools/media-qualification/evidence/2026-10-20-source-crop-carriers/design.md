# Next bounded proposal: SelectedPlacement crop-only empty carriers

Read-only source-derived proposal, 2026-10-20. No Cargo/runtime evidence for this case. Current integration remains frozen.

## Exact integration boundary

`crates/deadpan-core/src/insert_time/retimed/speed_update.rs::validate_retime_update` now shares a bounded `FrozenAudioLayout::project_scoped_with_support` projection between retained and freshly captured owners. Its `support` contains current intrinsic physical-local extent and meaningful ancestor Edit crops, while deliberately ignoring transparent Partition visibility. It does not clip the owner's selected audio crop.

Renderer equivalence:
- `crates/deadpan-plan/src/plan.rs` compiles `audio.selection = source.audio_mapping.selection_frames_with_offset(source.duration, source.audio_offset, project_rate)` and `audio.selected = matches!(mapping, SelectedPlacement { .. })`.
- `crates/deadpan-plan/src/audio_bound.rs::bound_at` intersects intrinsic/current meaningful support with `audio.selection` **only when selected**. `raw_domain` maps this current physical-local support through the retained lattice directly. It does not subtract a resume/local anchor from the support.
- `crates/deadpan-audio/src/stages.rs::read_bound_at` returns exact zeros before constructing the transfer if the resulting Root sample range or Point sample support is empty.

Smallest core extension: immediately after the existing shared meaningful-support projection, if the current owner is `NodeKind::Source`, `source.audio.is_some()`, and mapping is `SourceAudioMapping::SelectedPlacement`, compute the selection with the same public method and intersect:

    selected = mapping.selection_frames_with_offset(source.duration, source.audio_offset, rate)?
    support.start = max(support.start, selected.start)
    support.end = min(support.end, selected.end)
    if support.end <= support.start: continue

Then use this exact support in the already implemented bound `lattice.sample_boundary` or fresh RootRoundEven boundary equality check. This is O(1) extra owner work beyond the existing bounded projection. Retain the historical lattice and source mapping byte-for-byte. Do not clip Placement/Duration/FitBeat, do not use `source.edit_window` as a substitute, and do not alter the candidate visible-allocation guard. No query API, schema, DSP, native or semantic changes are required. `retime_update_input` retains structural eligibility; `validate_retime_update` remains the prospective rate authority.

Coordinates: the returned selected endpoints are **current physical-local project frames**, already shifted by audio_offset * fps / 48000. Intersect directly. Do not add the mapping start, subtract the audio offset again, convert endpoints to source samples, or subtract the retained resume boundary. This exactly matches `bound_at`.

## Concrete retained and fresh PCM fixture

Use NTSC30000/1001, F=5/8008 root frames per48k mix sample. Start with root `[Source2(no audio), stage, Source2(no audio)]`; stage is FollowSpeed Edit input `[0,100001)`, output1000; child Sequence has Source1 `tone` then silent Hold100000. Use a qualified44.1kHz Original asset and Source audio span `[3100,3116)` (16native samples), full audio mapping duration1 local frame. Its old native source step is9091/9100, inside supported bounds, so the negative control does not hide a native resampler failure.

Set offset=AudioSample(1). Let shift=F. Author:

    SelectedPlacement { start: -shift, frames: 1,
        selection: [1/2-shift, 501/1000-shift) }

The actual offset-shifted full mapping is `[0,1)`, selected crop is `[1/2,501/1000)`. Also repeat a control with offset=-1 and compensating authored start/selection, yielding the same actual local support, to catch double-shifting.

Public history after initial fixture admission:
1. InsertTime atroot0, duration1, no split IDs: captures the old Source lattice at rootorigin2, scale1000/100001.
2. InsertTime atroot4, duration3, pool sized by `pause_insertion_target`: stage starts atroot3, and cut is inside the silent remainder at child100001/1000. This makes fractional exact clocks; stage extent1003. `tone` remains an intact physical Source1 with no Partition ancestor.
3. Retained case: keep its binding. Fresh case: remove only tone's binding from the admitted fixture and prune orphan timing rows, like the existing partial-binding regression. Before editing, run `capture_unbound_audio_bindings` as a pure factory control and resolve its prospective tone lattice.
4. Request public SetRetime(stage, output1000000, FollowSpeed). This should be admitted after the support intersection, but current code should refuse its final retained-transfer validator.

Exact boundaries (round-even):

| carrier | full current raw support0..1 on old lattice | selected local1/2..501/1000 on old lattice |
| --- | --- | --- |
| retained oldorigin2 | [3203,3219), nonempty | [3211,3211), empty |
| fresh currentorigin3 | [4805,4821), nonempty | [4813,4813), empty |

Current physical scale is1000/100001. Candidate scale is1000000000/100301003, candidate selected allocation is [12789,12805), exactly16new mix samples. Both retained and fresh transfer are1003/1000000 <1/64. The full old carrier is nonempty; only selected support proves the unused transfer, so this specifically exercises the deferred gap.

PCM oracle: assert both complete old-carrier nonemptiness and selected old-domain emptiness before the command. With the fixed command, render all16new selected samples and an enclosing bounded window using the normal StageAudio path: every sample is exact digital zero, with no alignment, tolerance, or packet concerns. If convenient use a provider that fails on access for this window to prove no native source read is needed; otherwise require its source-read counter to stay zero. Preserve unrelated outside PCM, historical bindings, exact inverse and JSON roundtrip. Different request partitions must give identical output.

## Neighboring nonempty crop, same rate refusal

Change only selected end from501/1000 to3/5, with the same compensating offset. Old selected carrier becomes [3211,3213) for retained, [4813,4814) for fresh. Candidate selected range is [12789,14386). The transfer is unchanged1003/1000000 and must still refuse atomically at query and public apply; unchanged document/history and no processing should occur. This proves the new allowance is an actual empty-carrier proof rather than a Source-wide bypass. Existing inclusive1/64..64, successive-update and Partition-full-context tests remain required.

## Limits / non-goals

No wider Source edit admission, source slip/trim support, Repeat inside updated family, processing pitch, unrelated processing state, provider changes, or DSP limit adjustments. The proposed fixture and numbers are exact analytic source evidence; root must first execute the current-code failing witness before implementation or qualification claims.
