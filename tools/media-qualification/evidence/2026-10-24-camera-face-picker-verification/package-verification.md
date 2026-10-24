# Camera face-picker package verification

Status: no_findings.

Verified all 138 inventoried payloads, 6,157,873 bytes, plus files.json. Primary and all earlier audit files were unchanged.
Verified 135 encoding rows: 120 identity, 13 ordinary gzip and two TAR gzip. The TARs contain 39 regular members, no omitted files, and one retained checkpoint metadata pair with empty WAL and 32,768-byte SHM.
The ten qualified source snapshots, unexecuted metadata-only eleventh path, six build inputs plus the fixture, retained binaries, raw reports and earlier independent audit match their byte sources. No live .rs file or executable binary was copied.
Published raw results preserve the passing final checks and earlier failures/warnings. The actual movie, all 120 pictures, full zero-offset [0,192192) audio, exact portable authored document and 13-revision/seven-history-row database agree with the previous audit.
No collector, product, build, export, decoder or Git command was executed. Scripted positive faces, shared picture/DSP references and native physical-input/latency limits remain explicit.

Exact inventory, script, plan and prior-audit pins are in package-verification.json.
