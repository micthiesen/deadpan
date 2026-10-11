# Source-format combined edits, 2026-10-11

The real release test creates 40 profiled projects from 20 retained source
fixtures. Each uses typed cuts, a six-frame Retime of Original [4,8), two
plays with a three-frame silent gap, and a three-frame freeze at Edit 22.
Both Preserve and Tape pitch policies produce exactly 26 output pictures.
Expected source ordinals come from fixture-generator clocks and manually
flattened edits, independently of the production index and render plan.

All 1,040 picture checks pass. All 36 signal windows measure zero offset;
four video-only windows are silent and make no offset claim. Exact emitted
sample endpoints are 41,642 at 30000/1001, 24,960 at 50 and 20,821 at 60000/1001.
The release test takes 85.79 seconds after its build. Strict CLI all-target
Clippy and formatting pass. Runtime source, fixtures and policies did not change.

`source-identity.json` pins source, fixtures, generators and both executed
binaries. `runs/` retains every full quality report, emitted movie and publication
report. `artifact-inventory.json` hashes the complete scratch projects and
requests. `checks/` retains commands, logs, summary and the first failed run.
`compression.json` describes gzip files; `SHA256SUMS` covers all retained files.

The initial test helper used a filename containing a dot to construct a
revision ID and failed before editing or export. The helper now uses a valid
fixed ID prefix and keeps the filename as its report label. Review also corrected
the VP9 fixture's oracle to its authored alternating 60/40 ms intervals; it is
not the AV1 dropped-frame pattern. A post-run console summary initially tried
to take the maximum over optional level values, including null for silence;
this did not affect any verification report or the saved matrix summary.

Reproduce with the project's qualified FFmpeg prefix and Rust toolchain:

    DEADPAN_PREVIEW_EXPORT_KEEP=1 cargo test --release --locked -p deadpan-cli --test preview_export source_formats_preserve_independent_clocks_through_combined_structural_edits -- --nocapture

The original complete packages remain at the `root` in `checks/matrix-summary.json`.
No native window, acoustic/device playback, real-person footage, fresh bundle
or sanitizer run is claimed by these test-only changes.
