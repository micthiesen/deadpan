# Backend admitted edited views

Base: `41c540aab86af1414bfe73b1378694659bb6763d`. Owned sources are recorded in
`final-source-sha256.txt`. No native app, core, plan, audio, media or repository
documentation changes by this worker. No commit, push, schema change or UI use.

## Public API

`deadpan_store::slice_preview` exports:

```rust
pub struct SliceViewIdentities {
    pub empty_revision: RevisionId,
    pub view_revision: RevisionId,
    pub root: NodeId,
    pub paste: SlicePasteIdentities,
}
#[derive(Clone)]
pub struct AdmittedSliceSource {
    pub receipt: Arc<SourceQualificationReceipt>,
    pub original: OriginalMediaRecord,
}
pub struct AdmittedSliceView; // actual fields private; no public constructor or Deserialize

impl ProjectStore {
    pub fn preview_edit_slice(&self, request: &CommandRequest)
        -> Result<AdmittedSliceView, StoreError>;
    pub fn view_edit_slice(&self, slice: &CapturedEditSlice, identities: SliceViewIdentities)
        -> Result<AdmittedSliceView, StoreError>;
}
impl AdmittedSliceView {
    pub fn document(&self) -> &Arc<ProjectDocument>;
    pub fn placement_base(&self) -> Option<&Arc<ProjectDocument>>;
    pub fn capture_revision(&self) -> &RevisionId;
    pub fn sources(&self) -> &BTreeMap<AssetId, AdmittedSliceSource>;
    pub fn originals(&self) -> &OriginalImportHandle;
    pub fn generated(&self) -> &GeneratedReadHandle;
    pub fn matches_originals(&self, handle: &OriginalImportHandle) -> bool;
    pub fn check_live(&self, cancelled: &AtomicBool) -> Result<(), StoreError>;
}
```

`OriginalImportHandle::check_live(&AtomicBool)` is now public, and
`same_session(&Self) -> bool` compares storage/revocation Arc identities.
`SourceQualificationReceipt::asset_record(String) -> Result<AssetRecord, StoreError>`
is public for complete deterministic receipt-contract verification.

Playback `Snapshot::sources` is now `Arc<BTreeMap<AssetId, SourceEntry>>`.
`Snapshot::committed` still accepts a plain BTreeMap; old `Snapshot::proposed`
keeps its signature and rejects changed assets. Added:

```rust
pub fn proposed_edit_slice(base: &Snapshot, view: Arc<AdmittedSliceView>, draft: u64, change: u64)
    -> Result<Snapshot, SnapshotError>;
pub fn validate_edit_slice_view(&self, view: &Arc<AdmittedSliceView>)
    -> Result<(), SnapshotError>;
pub fn validate_original_proposal(&self) -> Result<(), SnapshotError>;
```

The last method separates strict original proposals from edited proposals even
when both catalogs are empty. `SnapshotError::MediaAdmission(String)` reports
the sealed view's revoked/cancelled media.

## Behavior

- Both store factories require the owning writable store. Placement accepts only
  SpliceSlice, SpliceSliceAt or ReplaceSlice and uses existing full command
  preparation. Standalone views apply pure SpliceSlice to an empty unity Sequence
  with supplied scratch identities and capture presentation basis.
- Full immutable-revision recapture, source/generated historical admission,
  revision freshness and document/request/patch bounds run before sealing.
  Neither factory writes history or generates IDs, compiles plans or decodes.
- Only the sealed output's qualified sources enter its exact receipt/original
  catalog. Receipt lookup does not reread the whole revision once per asset.
- Placement snapshots bind the actual supplied base Arc, sealed view Arc, output
  Arc, source-map Arc, session/content and Original-handle identity. Catalog
  verification is constructor-only; hot admission is constant-time.
- Edited-view source reads check live handles before/after warm or cold reads.
  Playback checks again before each batch, including cached/looped PCM that
  bypasses source lookup. Closing/reopening a store does not renew the old view.
- Existing ordinary Original and committed warm private-PCM semantics remain.
  The initial review wording was interpreted too broadly before reading the
  deliberate `source_provider_rejects_foreign_contracts_and_revoked_cold_handles`
  and `seek_reuses_private_pcm_but_a_new_session_must_reopen_sources` contracts.
  Root agreed to narrow revocation to sealed edited views; both existing tests
  retain their assertions and passed. The latter has only the necessary Arc-map
  constructor adapter. Linked-path-loss cache behavior is unchanged.

## Changed files

Production: store `src/{slice_preview.rs,lib.rs,original_media.rs,source_registration.rs}`;
playback `src/{sources.rs,preparation.rs}`.

Tests: store `tests/edited_slice.rs`, new `tests/edited_slice/preview.rs`,
`tests/{source_registration,generation_bundles}/edited_slice.rs`; playback
`src/tests/proposed.rs`, new `src/tests/proposed/edited_slice.rs`; Arc-map adapters
in `src/tests.rs`, `src/tests/{audio_range,sound,source_voice}.rs` and
`src/tests/source_voice/{events,routed}.rs`.

## Verification and exact failures

All Cargo commands used environment
`DEADPAN_FFMPEG_PREFIX=/tmp/deadpan-ui-ffmpeg/prefix`, explicit
`rustup run 1.97.1`, and `--locked`.

1. `cargo check -p deadpan-store -p deadpan-playback --locked`: exit 0,
   `check-01.log`. Initial production implementation; before final narrow-liveness
   adjustment and new tests.
2. `cargo test -p deadpan-store --test edited_slice --test source_registration
   --test generation_bundles --locked`: exit 0, 8 + 15 + 17 = 40 passed,
   `store-tests-01.log`. Final store source.
3. `cargo test -p deadpan-playback --lib --locked`: exit 101, 59 passed / 1 failed,
   `playback-tests-01.log`. New test mistakenly captured impulse-only media's
   silent interval [1,4), then required a nonzero sample. Failure was exactly that
   nonzero assertion, before equality comparison. No production fault was found.
4. Changed only the new test's capture to [0,3), including the documented opening
   impulse at source sample 100. Added strict original-versus-edited token method
   requested by the picture worker and the empty-catalog assertion.
   `cargo test -p deadpan-playback --lib tests::proposed --locked`: exit 0,
   7 passed / 53 filtered, `playback-proposed-02.log`, final playback source.
   The corrected copied PCM is nonzero and bit-identical to later committed PCM.
5. `cargo clippy -p deadpan-store -p deadpan-playback --all-targets --locked --
   -D warnings`: exit 0, `clippy-01.log`, final owned source. Finished in 2m31s.
6. Explicit owned-file Rustfmt and `git diff --check -- crates/deadpan-store
   crates/deadpan-playback`: clean. No whole-tree formatter.

Decisive coverage: Source capture after registration Undo; Generated provider after
last Hold deletion; forged capture/history/asset/artifact rejection; standalone
ownership/root-bus exclusion; all three placement forms exactly equal ordinary
preview; no durable writes; public field/Arc/foreign/reopened-handle rejection;
512-entry catalog checked only once across repeated hot reads; warm edited-source
revocation; deterministic closure immediately before publishing a reused loop
batch with no source lookup.

## Retained limits

Factories are writable-store and same-project only. Legacy/unqualified media keep
existing explicit decode failures. Generated store tests prove accepted artifact
and handle authority using the existing synthetic bundle, not generated-video
decode quality. Native endpoint rendering, UI lifecycle, commit recovery and a
whole-workspace gate belong to other workers/root. No new subprocess launches.
Live checks govern new preparation/publication; existing transport cancellation
still stops already queued output after a session change.
