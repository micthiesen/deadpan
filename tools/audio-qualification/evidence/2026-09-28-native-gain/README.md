# Native gain evidence

See the [qualification record](../../../../docs/qualification/native-gain-2026-09-28.md)
for implementation, review corrections, verified boundaries and remaining work.

Command JSON records invocations, source manifests, start times, terminal exits
and elapsed time. Matching gzip logs preserve complete output. Source inventory
filenames hash the uncompressed JSON. Documentation/evidence updates after a run
are not claimed as compiled source. Inventories identify exact Cargo artifacts.

`summary.json` derives command test counts and replay assertions without adding
overlapping populations. Failed runs retain their original outcome; passing
focused follow-ups do not rewrite those outcomes. Durations including compilation
are not product latency measurements. Replay timings describe their own workloads.

Compressed replay JSON retains all diagnostics but most referenced PNGs are
intentionally omitted. Selected actual failures and final default/minimum gain
captures remain under `images/`. Native evidence, when present, is separate.
No private project, media snapshot, executable or cache is published here.

Gain replay uses production writer/media/GPU paths and injected typed audio
delivery. Real qualified PCM is covered separately by playback tests. Physical
keyboard layouts, OS IME, VoiceOver, listening, long-media response and encoded
export are not established by these reports.

`manifest.json` hashes every retained file except itself.
