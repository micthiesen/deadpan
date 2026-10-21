# Selected Source support boundaries: scratch draft

Repository and production unchanged. No Cargo, Git, or runtime execution. Candidate full owned test file: `source_crop.rs`; exact unified diff against current repository file: `support-boundaries.patch`.

The patch leaves the existing three tests and `crop_history(end,offset,fresh)` call contract unchanged. Its wrapper delegates to `crop_history_window(mapping_start,crop_start,crop_end,offset,fresh)`. New tests each run retained/fresh bindings and both signed offsets1/-1 with exact compensation. Same natural160samples@44.1k, public InsertTime0/1 then InsertTime4/3, stage2000->2003, requested output1000000, supported native step14700147/320000 and retained-transfer step2003/1000000.

1. `speed_update_selected_crop_is_clipped_to_intrinsic_source_before_raw_allocation`
   Natural full mapping starts199/200, duration16000/147147, selection[999/1000,103/100). It is valid within that full mapping even though it extends past the Source's intrinsic end1. Renderer intersection with meaningful intrinsic[0,1) gives[999/1000,1).
   Retained full selected old samples[3235,3236), fresh[4837,4838), both NONEMPTY. After intrinsic intersection the old sample ranges are[3235,3235) and[4837,4837), both empty. This distinguishes the correct intersection from checking the full selected crop alone.
   Candidate full physical Source samples[4805,20797); full selected candidate[20781,21276), clipped candidate[20781,20797), exactly16samples. Query/public update must admit because the old raw carrier, properly intersected, is empty.

2. `speed_update_selected_crop_outside_intrinsic_source_keeps_empty_raw_domain`
   Natural full mapping starts499/200, same duration; selection[5/2,2501/1000). Its full mapping end is above2.6037, so the crop is valid, but the entire selection lies outside intrinsic[0,1). The literal max-start/min-end intersection is[5/2,1), nonpositive, so renderer `AudioBoundDomain::Empty` applies before transfer.
   Old selected sample ranges retained[3283,3283), fresh[4885,4885); candidate selected[44784,44800), entirely beyond physical Source's[4805,20797). Query/public update must admit. The test preserves the nonpositive range proof without normalizing to an invented endpoint.

Both new tests assert unchanged input JSON, Source recipe/offset/mapping, retained Source binding (or installed fresh binding), all old immutable timing tables, JSON roundtrip and exact inverse. Public outside-clock suffixes may acquire their required reanchor. Optional witness JSON uses the existing parent helper.

Boundaries above were independently calculated with Python Fraction + exact round-even. Scratch Rust source was rustfmt-checked only. Parent-owned unique filters: `speed_update_selected_crop_is_clipped_to_intrinsic_source_before_raw_allocation`, `speed_update_selected_crop_outside_intrinsic_source_keeps_empty_raw_domain` (or full `retimed::speed_update::source_crop::...` with --exact).
