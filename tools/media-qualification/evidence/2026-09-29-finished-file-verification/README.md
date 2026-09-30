# Finished-file SDR verification evidence

Base: `4387a88a7c110c8ed86ddc98099a8f248da7aa59`. See the
[qualification](../../../../docs/qualification/finished-file-verification-2026-09-29.md)
and [boundary contract](../../../../docs/FINISHED_FILE_VERIFICATION.md).

## Results

- Workspace: 2,222 tests passed, zero failed or ignored; Clippy with warnings
  denied and formatting passed.
- Native C ASan/UBSan: 241 selected tests passed. The final normal and
  instrumented workers each verified seven fresh MP4s, 304 pictures, 390,695
  authored audio samples and 21 complete GOPs.
- Independent content readers passed 912 planes containing 116,984,106 component
  codes, complete PCM comparisons and 18 exact encoded-file marker observations.
- `native-artifacts.tar.gz` contains 365 regular files, 638,784,903 uncompressed
  bytes. `archive-members.json` records each size and SHA-256. `archive-audit.json`
  records full readback, not just a successful compression command.

## Reading the evidence

| Files | Meaning |
| --- | --- |
| `summary.json` | Counts, extrema, commands, exit codes and source comparisons. |
| `workspace-tests.*`, `workspace-clippy-final.*`, `format-final.*` | Final repository gates. |
| `sanitized-tests.*`, `sanitizer-report.json`, `sanitizer-test.log` | Instrumented package tests, flags, runtime and binary hashes. |
| `metal-current-report.json.gz`, `metal-current-artifacts.json` | Fresh project/GPU/encode/verification run and exact private executable copies. |
| `decoded-current-report.json.gz` | Independent complete FFmpeg/AVFoundation content and clock observations. |
| `verified-final-*`, `verified-sanitized-*` | Final worker reports and before/after executable identities. |
| `source-*.json.gz`, `source-changes-since-check.json`, `final-source-binding.json` | Complete source inventories and final binding. |
| `*-review.md`, `review-dispositions.md` | Independent source review, corrections and final execution disposition. |
| `retained-*`, earlier build/lint journals | Failed development attempts and the eventual passing retained-file run. |
| `file-manifest.json` | SHA-256 and size of every other file in this evidence directory. |

Archive directories retain fresh MP4s and manifest sidecars, direct I420/PCM
references, independent reader results, both final verifier inputs and failed
attempt inputs. `project-inputs` holds coherent SQLite backup-API snapshots and
retained assets; live database main files/WAL/SHM were not copied as snapshots.

`run-metal.py.txt` and `run-verifier.py.txt` select the captured executables and
check their hashes after execution. Build/check journaling used the retained
[`run-native.py`](../2026-09-29-generated-pictures/run-native.py).
`package-evidence.py.txt` records packaging and archive verification.

## Limits

All final repository gates, instrumented tests and final verifier runs bind to
source inventory `ad271d24c7ba5226183f89e6a4e955014b81d8f7d361e94fd5b4751422874820`.
The fresh encode predates the small Rust/harness/test changes listed in
`source-changes-since-check.json`; no native encoder/decoder C code changed after
that run. Final reinspection uses those same retained fresh candidate bytes.

ASan/UBSan instruments native C adapters and target C dependencies. Rust, C++ DSP
and the separately built FFmpeg libraries are not instrumented; leak detection
is disabled. These runs do not establish sustained performance, broad hardware
coverage, clean-machine distribution or full product export. The instrumented
reinspection exercises decoding, not a new instrumented hardware encoding run.

The three checked-in test movies replay actual encoded bytes under a synthetic
document binding. They prove verifier/host behavior; fresh project content
qualification is recorded separately above. Cancellation after reported progress
proves admission rejection and candidate retry, not active decoder interruption.
No destination publication or native Render workflow is implemented here.
