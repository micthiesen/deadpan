# YouTube confirmation thumbnail evidence

See [qualification](../../../../docs/qualification/youtube-thumbnail-2026-10-11.md)
for results, failures, bounds and verification scope. Source changes are bound
by `source-identity.json`; executed binaries are identified by the replay,
real-thumbnail and bundle reports. `manifest.json` hashes every retained file
except itself.
Large JSON reports and logs use deterministic gzip; `compression.json` records
their original lengths and SHA-256 hashes, which the collector also verifies.

- `checks/` retains commands, logs and exit statuses, including the failed
  workspace gate, initial compile failures, layout failures and their follow-ups.
- `replays/` retains all JSON reports and selected actual offscreen Metal PNGs.
  Reports can refer to other captures retained only in the scratch directories.
  The scripted image is owned procedural test data, not the remote video's art.
  In `affected-replays`, `youtube-103.png` records the first-frame long-title
  defect missed by the settled-frame checks. The final replay checks it directly.
- `real-thumbnail/` and `release-thumbnail/` retain the production client's
  actual fetched/decoded thumbnail and exact image/executable hashes. The
  source is Blender's CC-BY *Caminandes 2: Gran Dillama*, video `Z4C82eyhwgU`.
- `bundle/` retains release identity and dependency/license inventory;
  `bundle-binaries.json` identifies each executable in the final app.
- The collection script records source hashes and verifies its file manifest.

Large binaries and complete intermediate captures remain under
`/tmp/deadpan-youtube-thumbnail-20261011`; the final app is
`/tmp/deadpan-youtube-thumbnail-final-bundle-20261011/Deadpan.app`.
The earlier `/tmp/deadpan-youtube-thumbnail-bundle-20261011/Deadpan.app` passed
bundle checks before the final sheet-position fix and is superseded. No real
app-job project is retained: that opt-in test creates and validates it in a
temporary library, which its normal teardown removes.
