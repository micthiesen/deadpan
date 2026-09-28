# Measured gain waveform evidence

See [qualification](../../../../docs/qualification/gain-waveform-2026-09-28.md).
Command JSON records source identity, invocation, terminal outcome and time.
Gzip files preserve complete original logs, source inventories and replay JSON.
Source filenames hash their uncompressed inventory. Failed runs remain failed;
passing follow-ups do not rewrite those outcomes. Test populations overlap.

Only inspected selected captures are retained. Most raw replay screenshot links
are intentionally absent. No private project, media snapshot, executable or
cache is published. Waveform values in the replay come from actual qualified
PCM; the explicitly labelled failure tests inject only failure state. Comparison
delivery feedback is injected for layout checks. Those checks do not establish
acoustic, device, physical input or display-latency acceptance.

The scripts use task-specific scratch paths from the original execution. Adapt
those paths before reproducing. manifest.json hashes every file except itself.
