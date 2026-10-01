# Native copied pictures implementation

2026-09-30, shared checkout. No Cargo or UI run by this agent. Parent owns first app compile and integration verification. Rust 1.97.1 owned-file rustfmt (`skip_children=true`) and `git diff --check` pass.

## Implemented API

- `Work::EditedProposed { base, snapshot, media, frame }` uses exact admitted snapshot/media seal and base identity.
- `Work::Copied { view, frame }` compiles the standalone document privately and uses local project-frame coordinates.
- `EndpointIdentity.source: EndpointSourceId::{Original { asset, qualification, in_frame, out_frame }, Copied(CopiedViewId)}`.
- `EndpointInput::{Original(Arc<Workspace>), Copied(Arc<CopiedView>)}`; `EndpointWorker::submit(identity, input)`.
- One bounded plan cache across Original proposal, edited proposal, or copied view, keyed by exact immutable Arcs. Existing one retained decoder reused.
- `PictureMedia::{Committed, Slice}` changes only media authority/session access. The materialized document supplies the asset contract and plan; no fabricated Workspace/history.
- Strict `Work::Proposed` requires backend `Snapshot::validate_original_proposal()` before normal exact-base admission. Empty Background catalogs cannot bypass the distinct edited seal.
- New warm revocation applies only to admitted Slice views. Ordinary committed/Original proposal private warm decoders preserve existing close behavior. Generated before/after liveness remains unchanged.

## Presentation

- `Location::Copied { source: CopiedViewId, frame }` carries historical source identity plus local owner clock and never qualifies for Camera/committed target.
- Main label is `Showing copied Edit frame N` using source range start + local frame + 1.
- Endpoint targets track accepted identity/raster/canvas/background independently of decoded reply.
- Composed render uses canonical canvas, provider-to-root layers, gap flag and captured context; raw Original path stays raw.
- Viewport fits canonical canvas aspect before target allocation. Failed resize preserves accepted registration and clock. New pair labels wait until both accepted identities match.
- Explicit Background transitions to ready black without a fake RGBA source. No submission on `will_discard`; at most one endpoint GPU submission per call and shared renderer busy deferral.
- Existing Original accessibility strings retained; copied accessibility uses `First included copied Edit picture, frame N` / `Last included copied Edit picture, frame N`.

## Files

Production: worker.rs, worker/proposed.rs, worker/endpoints.rs, new worker/slice_view.rs, presentation.rs, preview/splice/pictures.rs.
Tests: worker/endpoint_tests.rs and worker/proposed_tests.rs migrated for API; worker/project_tests.rs adds focused module and reusable fixture construction; new worker/edited_slice_tests.rs; presentation/tests.rs; pure endpoint display tests within pictures.rs.

## New decisive assertions

- Real CFR source endpoints select ordinals 20 and 24, exact PTS `ordinal * 1001`, exact metadata and RGBA equal to raw Original decoding.
- Owner frame-center clocks are 20.5 and 24.5 over original duration 120; selected Source scale 2 retained. Historical ancestor scale 3 and destination scale 4 excluded from standalone endpoints. Proposed destination has selected scale 2 followed by destination scale 4.
- Main copied view and endpoint worker return same framing and exact decoded bytes for local 0/4.
- Wrong CopyId request rejected against supplied view.
- Freeze local 0/1 returns identical bytes with owner positions 0.5/1.5, retains independent captured 102×62 geometry; Background Out completes with no frame and canonical 320×180 canvas.
- Cancelled copied request and local frame equal to exclusive duration fail.
- After Undo removes current asset catalog, both standalone copied and edited proposal decode admitted historical Original. Changed snapshot source map and swapped media seals fail. Old strict Proposed rejects edited snapshot.
- Closing writer rejects warm edited/copy work; reopening cannot revive old capability. Existing strict Original warm-after-close test retained.
- Pure presentation tests cover copied captions, same pixels/different owner clock, failed GPU allocation preserving caption, stale refinement success/failure rejection, Background display, and Camera exclusion.
- Pure endpoint state tests cover two-slot readiness, Background readiness, mixed identity rejection, retained raster after failure and odd canvas aspect.

## Limits / pending checks

All new tests remain unrun until parent's serialized app check. These tests establish decoded media and canonical metadata plus pure presentation state. They do not establish painted GPU pixels, physical display behavior, or final native replay. No new decoded generated-Hold native fixture exists here; generated provider continues through the existing qualified CLI reader and admitted historical handle. Store/backend tests own generated historical admission evidence. Parent owns native QA/replay and complete workspace gate.

## Shared-check follow-up

Parent's first app compile found one test-only use of Snapshot::clone; test now constructs a fresh genuine admitted Snapshot before tampering. Parent's first test run reported 357/359; own Freeze fixture failed because CapturedCanvas requires even dimensions. Fixed fixture from101×61 to102×62, leaving the pure viewport aspect test at101×61. Parent owns rerun. Original accessibility strings retained for existing replay.

Also fixed stale endpoint error presentation: only errors whose decoded identity equals expected identity appear in the placeholder/error line. Added pure test. Same-identity error can coexist with the accepted retained pair.

Read-only UI/service review sent parent three concrete issues: pending copy supersession before sound-focus early returns; successful placement with failed refresh clearing old Edit range; coalesced pending copy completion overwriting saved-refresh reopen guidance. Parent owns those fixes and replay.

## Warm admission performance correction

Independent review found that added media_source receipt.asset_record reconstruction derived full measured audio extents on every picture. Moved complete reconstruction into slice_view::check_catalog on exact immutable view cache misses; hot media_source retains the original asset/qualification/content/original checks. Service catalog and proposal constructors still validate full contracts, and each edited/copy view binds the exact sealed document and receipt pointers.

New real-media regression `full_receipt_contracts_are_checked_once_per_view_not_per_warm_picture` asserts a measured audio index and counts reconstruction at its production admission call: five copied seeks=>1, changed immutable copied view=>2, five edited proposal seeks=>3, with exact20..24 source ordinals. Existing forgery/seal and liveness tests remain. No Cargo by this agent. Final owned Rust1.97.1 rustfmt check and whole diff whitespace check pass. Parent reported399 optional app tests passed on preceding code; new focused counter test remains for parent's run.
