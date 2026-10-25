# Scoped whole-owner Follow creep

Twelve owned paths are frozen in worker-scoped-final-freeze.json. Exact final source copies are under worker-cli-unit-1-source; every attempted check also saved its exact pre-run .rs.txt copies. No source or formatter change followed the final CLI integration/unit passes.

## API and production

WholeFollowOwner replaces OrdinaryFollowOwner and Shot.whole_owner replaces ordinary_owner. This is a Rust marker rename, not serialized authority or wire data. The five actual owner kinds remain Source/Hold/Sequence/Repeat/Retime.

The ordinary planner keeps its existing selection/direct-parent guards. zoom_scoped calls unchanged validate_scoped_context, resolves current state/duration, refuses an empty owner, then classifies the actual scoped target kind before building the Follow transition. The existing EditScoped requirements, allocation/reservation, prepare/map_target/map_instance and next staged context path remain intact. The authority function is byte-for-byte identical to authority-before.rs.txt; its hash is frozen. Default, Visual, missing/mismatched/outside-root/Original and empty authority cannot acquire a marker through this change. Existing animated paths still refuse. No schema, raw validity, clock, numeric, isolation or structural algorithm was changed. No old API references remain under crates.

## Core proof

The builder covers all five WholeFollowOwner variants with separate bases, retained entry clocks, OwnerOutput creation, explicit-from, full/interior range and missing-marker/animated-path refusals. Ordinary semantic guards remain covered.

The new scoped semantic case exercises all five actual kinds below two explicit Repeat plays, including Retime as selected owner. It checks exact requested isolation pool counts, unchanged shared templates, copied marks, mapped target/presentation validity, independent live/fallback endpoint-interior values, only the selected concrete branch acquiring a transition, Compound replay, exact inverse and JSON round-trip. Reapplying animated Keep refuses before allocation. Default at either level, missing Repeat steps, mismatched presentation, complete owner Visual range, missing scope/direct parent and foreign captured root refuse before allocation. Existing scoped authority/noop/allowlist and temporal/audio semantic tests also pass.

## CLI real-project proof

The new integration loops over Source/Hold/Sequence/Repeat/Retime as six-frame owners. Each is inside two concrete all-Play Repeat choices with a nonunit Tape Retime ancestor: inner12 maps to18 output, outer2 plays plus a two-frame Background gap gives38 frames; the original120-frame suffix remains. Outer Play2/inner Play2 selects global[29,38), whose nine project frames sample owner coordinates(2k+1)/3 over the complete six-frame owner clock.

Targets interpolate .4→.6, retain Lost .6, then correct to .7. The Retime owner introduces an additional2/3 provider map. Sequence has a Background fallback and a late live crop clipping the target; Hold retains a captured crop clipping its frozen target. Repeat owns a frozen gap and separate9/8 escalation on its second play. The source aspect comes from measured qualification. Independent integer half-even Q32 oracles check every selected owner's live/fallback scale; independent exact mapped SourcePoints, measured ordinals, normalized target interpolation and inner escalation centers are asserted for all nine views of every owner.

A shared test-only proof helper reads the actual Compound request from durable SQL history, verifies every allocated clone ID against isolation records/requirements, maps every physical picture/layer address, validates canonical clone contents and unchanged retained templates, replays the whole Compound exactly and restores the full inverse. It compares all picture/provider/captured/caption/local-clock/gap fields and full canonicalized audio spans. Only the selected pose changes, except authored-unchanged ancestor Follows that must reproject through the edited view; those use independent affine/clipping/Q32 composition checks inside selected frames. All outside views remain strict. Clone identity mapping is verified before semantic comparisons, not ignored wholesale.

Named dry-run leaves complete SQL state unchanged and predicts actual selected framing/instruction sequence. Templates and unrelated plays/gaps retain semantics. Durable Undo/checkpoint/reopen/Redo restores exact documents/register bank. Staged Keep smash plus named call maps the already isolated target and re-resolves a new invocation target/base in a different concrete play, with one exact Undo. Inline Apply verifies the native-dot shared semantic boundary on fresh current authority. Reapplying animated Keep refuses, and Default/Visual/missing/forged contexts refuse atomically.

Older Hold/Sequence/Repeat/Retime explicit-Play negatives now have positive transition, physical clone/render/audio preservation and exact Undo checks. Genuine Default and forged/missing scope negatives remain. The renamed Hold test reflects its positive scoped replay.

The changed CLI resolver unit test proves Source duration/current Follow remain owner120 beneath a Retime and an explicit second Repeat play, despite a different project clock and deleted linked Original; the WholeFollowOwner builder retains its separate live2/fallback3 bases.

## Actual checks

- cargo test --locked -p deadpan-core --lib follow_creep: session20455, exit0,3pass.
- cargo test --locked -p deadpan-core --lib semantic::planner::tests::scoped_framing: session30216, exit0,59pass. The new scoped case overlaps those filters, so61 unique Core cases.
- cargo test --locked -p deadpan-cli --test project_commands macros::framing: final session58359, exit0,16pass,9.20seconds.
- cargo test --locked -p deadpan-cli --lib semantic_framing_uses_live_follow_pose_at_retimed_source_clock_instead_of_fallback: session17224, exit0,1pass.

Two initial CLI oracle failures are retained verbatim with .rs.txt snapshots and raw streams. Attempt1 had11pass/5fail at private-leaf versus outer revision equality. Attempt2 had15pass/1fail at an ancestor Follow's legitimate one-Q32-unit reprojection. Both corrections affect only the shared test proof helper. There was no production or clock contract defect and no production change after the initial scoped marker admission.

All runs retain exact argv, qualified FFmpeg/shared Cargo target environment, timestamps, terminal exits, binary observations and owned/named/global source maps. Twelve owned paths, the nine named dependency files and all22 complete global listed paths remained unchanged during every run. These are the listed source paths and named basis only, not an all-product source snapshot. No old output is overwritten.

Cargo is released, with no active worker process or owed check in this lane. CLI uses qualified video-only registration; unchanged silence audio-plan semantics and measured index assertions are not emitted PCM/pixel/export evidence. Root and Plan/native owners retain review, broad/strict/release, real native/media qualification, Git and publication.
