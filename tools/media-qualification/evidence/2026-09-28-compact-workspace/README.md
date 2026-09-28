# Compact workspace evidence

See [qualification](../../../../docs/qualification/compact-workspace-2026-09-28.md).
Command JSON records source identity, invocation, terminal outcome and time.
Gzip files preserve complete original logs, source inventories and replay JSON.
Source filenames hash their uncompressed inventory. Failed runs remain failed;
passing follow-ups do not rewrite those outcomes. Test populations overlap.

Only inspected selected captures are retained. Most raw replay screenshot links
are intentionally absent. No private project, media snapshot, executable or
cache is published. Playback feedback is injected for layout checks; it does
not establish acoustic, device, physical input or display-latency acceptance.

The commands reuse the native-gain evidence run.py, feature_tests.py and
verify_ui.py helpers. The compact continuation preserves completed app tests
and independent visual results. manifest.json hashes every file except itself.
