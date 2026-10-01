# Endpoint fixture review

The core fixture corrections preserve the intended absent-audio and legacy-wire tests. No production correction is indicated by these fixture failures.

- `tests/audio_bindings/source_endpoints.rs` now supplies a declared picture-only Stream and matching asset. Source duration, crop geometry, audio `None`, `FitBeat`, zero audio offset, Independent link, captured clocks and literal timing expectations are unchanged.
- `legacy_audio_binding_v36.rs::endpoint_tests::fixture` makes the same correction. Qualifying the two calls as `super::validate` selects the intended legacy binding validator; the positive old-wire assertion and rejection of default, endpoint, null and escaped `anchor` fields remain meaningful.
- `FrozenAudioLayout::capture` derives Source audio placement only from `source.audio`. These corrected Sources still capture `FrozenAudioKind::Source { placement: None }`, while retaining the positive structural allocation required by the endpoint primitive. The declared video asset is core fixture metadata, not measured media qualification.

Two other pending fixtures require this same admission correction, without replacing the Source with a Hold:

1. `audio/src/bound_reads/source_origin/endpoints.rs::absent_source_audio_keeps_endpoint_clock_without_reading_media` clears audio on an inherited Blank Source. It still needs a declared picture-only provider and asset.
2. The frozen `staged-trim-root-sound-audio/root-sound-trim-audio.patch` constructs its picture-clock Source as Blank plus audio `None`. Correct only that fixture provider/asset when integrating. The frozen patch was not changed.

## Slice chronology failure

The final `local_boundary == 4` assertion in `captured_slice_renames_endpoint_aliases_on_two_independent_pastes` is incorrect. `edit_slice::capture_audio` captures the current owner placement and calls `insert_time::composite::append_steps`, appending an AllocationEntry step with the selected project window. The existing SourceEnd step remains first. `audio_binding::resolve_owner` evaluates both steps in order. Alias renaming preserves the historical layouts; neither paste adds another step to its imported bindings.

For the current fixture, the historical and capture clocks are identical: lead 1, Source allocation 0..4. At 30000/1001 fps, samples per frame are 8008/5. SourceEnd advances from local 0 to 4 by `B(5)-B(1) = 8008-1602 = 6406` samples. Capture then returns from local 4 to 0 by -6406 samples. Both pasted bindings should have exactly two reanchors, `[SourceEndpoint(End), AllocationEntry]`, and final resume `{ local_boundary: 0, reference_local_delta: 0 }`. The endpoint-only resolution remains boundary 4 with delta `6406*5/8008`.

A stronger control makes chronology observable rather than merely changing 4 to 0: retain the historical layout `old(1, 0, 4)`, but construct `before` by binding that state to current `old(2, 0, 4)`. The first step still adds 6406 samples; the capture step subtracts `B(6)-B(2) = 9610-3203 = 6407`. Both independently renamed pastes must resolve to boundary 0 and delta `-5/8008` local frames, exactly one sample early relative to zero. Removing only the historical endpoint step from an otherwise equivalent captured binding resolves to boundary 0/delta 0. Assert the two-step order, distinct historical aliases, exact phase, negative control, and existing inverse equality. This tests retained endpoint history through capture and two pastes without changing production behavior.

Read-only source inspection only. No Cargo, native processes, or repository edits were performed.
