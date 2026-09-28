# Native room-tone evidence, 2026-09-27

See the [qualification](../../../../docs/qualification/native-room-tone-2026-09-27.md)
for implementation, review corrections, measured results and acceptance limits.

- Command records retain exact arguments, launch time, base commit, source
  manifests, duration and terminal exit. Logs include development failures.
  One owner ran Cargo and GPU checks serially; quiet work was not restarted.
- Source manifests hash tracked and untracked build sources and all files under
  `crates/` and `native/`. Documentation and evidence do not change that identity.
  Edits were finishing during the first development visual build, so its launch
  manifest is not proof of the compiled source. The final integrated gates ran
  on frozen sources; each replay also records its actual executable hash.
- Compressed reports preserve every assertion, semantic frame, timing sample
  and metadata field losslessly. `summary.json` extracts results, timing summaries
  and capture mappings. `manifest.json` hashes retained files except itself.
  These hashes identify evidence bytes, not release acceptance.

The six PNGs show prepared and edited source ranges plus the saved Hold inspector
at 960×640 and 1280×820. Each maps to its report, frame and checkpoint in the
summary. Other report screenshot paths refer to scratch outputs. Intermediate
image quotas preserve semantic frames and named captures. Performance replay
excludes screenshot readback from measurements.

The UI replay uses production widgets, routers, project services, decoded fixture
media and Metal. Its playback delivery is simulated. Separate decoded-PCM and
fake-device tests verify actual selected-source output, phase, endpoints and
loops. Physical keyboard/IME input, VoiceOver, display color, representative
ambience listening, full-size performance and encoded export remain unqualified.
