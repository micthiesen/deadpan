# Expanded processing Hold-duration PCM controls

2026-10-20. Test source frozen for root-owned Cargo. No runtime results claimed here.

Owned files: `crates/deadpan-audio/tests/stages/processing_hold_duration.rs` and child `processing_hold_duration_output.rs`. Child registered only from its parent. All PCM comparisons are scalar first-mismatch diagnostics over every sample/channel; suppression masks are independently compared at every sample. No alignment or tolerances.

## Direct point inputs

Input sample unit is 5/8008 frame. Direct natural 44.1k source step147/160. Source starts3100.

| input selection | Hold frames | entry | tape end | old-grid current local allocation length | retained phase |
|---|---:|---:|---:|---:|---|
|0..8|1|6407|12813|6407|3100|
|0..8|2|8008|12813|4805|3100|
|0..8|3|9610|12813|3204|3100|
|1..8|1|4805|11212|6406|3100+441/800|
|1..8|2|6407|11212|4805|3100+441/800|
|1..8|3|8008|11212|3203|3100+441/800|

Native Source ends are ceil(3100 + visible_frames*1471.47):8986 for4frames,7515 for3,6043 for2. Current selected crop governs resampler input support. Crop1 grown/shrunk cases each have one live final point beyond retained old-grid length, so sample11211 is separately zero/suppressed. Complete old and current tapes use these finite native crops and retained lengths.

## Affine operand

Inner affine output6/map0..8 gives local slope3/4. Old speech entry15/4 ->6006. New entry9/2 ->PointCeil7208, not rootRTE7207. Source step49/40. Selected operand ends at6 ->9610. Historical current local2 allocation old[15/4,21/4) ->[6006,8409),2403points; live[7208,9610) has2402points. No exhausted final point. Current native end6043.

## Prior cropped resume

Source on input selection1..8 has original phase3100+441/800. Existing resume5/8008 frame adds one input sample147/160, giving3101.47=310147/100. Two edits Hold2->1->3 must each resume current entry at the same literal phase; saved lattice/resume remain, reanchors retain their chronological prefix.

## Fractional output, independently rebuilt DSP

Output binding resume5/24024 local frame =1/3 sample. Before and after binding must be exactly equal. Both OLD and NEW full inputs are independently generated, then directly processed by canonical DSP. Preserve rate2, pitch0 prepares6407PointCeil points, but retained physical root support is0..6406 by RoundEven. Shift rate1,pitch3 prepares/root-support12813 points. No old/new processed PCM equality.

Prepared silent prefix: Preserve old4004/new4805; Shift old8008/new9610. Mask before interpolation. Independent RootSignalTransfer uses root_at_anchor1/3, signal_anchor0, step1, full physical root support. Actual public DomainSignalTransfer of live bound raw_domain must equal this independently computed transferred PCM and mask. Consuming StageAudio output must equal it after separately applying current output policy. Old baseline runs first. Alternate preparation/read schedules compare every output sample exactly.

## Opaque moved nested processor

Outer Shift+3 selected input0..8. Nested Shift+3 output2/mapping0..2 follows lead3/Hold2. Capture old bindings, then Hold2->3. Nested output belongs to outer processor's PointCeil clock, old[8008,11212),3204points; new[9610,12813),3203points. Nested intrinsic Source belongs to its own input clock with phase3100 and native crop3100..6043; binding must remain exactly unchanged. Independent full nested DSP is3204 points; live outer input consumes first3203, with silent prefix9610. This tests opaque output movement without flattening its intrinsic input onto the outer grid.

## Repeat/default/owned/room-tone follow-up

Two separate Preserve and Shift tests in new `processing_hold_duration_repeat.rs`. Old input layout: lead0..1; default pause1..3/Source3..5; room gap5..7; owned pause7..9/Source9..11; room gap11..13; last default pause13..15/Source15..17; suffix17..21. Fixed input0..18. Both unbound and partial historical gaps+owned Source variants exercise default Hold and owned Hold grow/shrink independently. Full OLD/current inputs and suppression are compared against independently supplied layouts and canonical DSP recipes; alternate read chunks compare every point.

Literal PointCeil entries: firstdefault4805,owned14415,lastdefault24024,suffix27228, endpoint28829. Source phases:3100+147/800;9000+441/800;3100;15000+147/200. Owned entry distinguishes rootRoundEven14414 fromPointCeil14415. Native Source crop ends use current visibility with exact147147/100 source samples/frame; no old finite-context-prefix assumption.

Room source512..833 at44.1k has321native cells ->17120/49 mix samples, stored350. Canonical RoomTone output2frames has3204 intrinsic points. Gap0 old frame5 starts8008 with localphase0 and physical3204points; gap1 oldframe11 starts17618 with localphase2/5 and physical3203points. They do not share phase or physical length. First gap becomes3203 livepoints on either default resize. Second gap at newframe10 has3204 livepoints against old3203, so its final point is zero/suppressed; at newframe12 live3203 matches old3203. Source exhaustion is independently bounded by old per-owner current-length allocation, with terminal masks kept separate from finite resampler crop.
