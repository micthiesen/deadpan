# Accepted AI media portability, 2026-10-07

DP-19 is complete within specification §29.1. Reference tracking, verified
objects, reader locks, explicit cleanup and portable copies are implemented
in [Storage](../STORAGE.md). This run adds a real-model accepted artifact to
the existing synthetic-worker, corruption, interrupted-write and history
tests. Other-Mac, File Provider eviction and physical power-loss checks remain
on [To verify (owner)](../REQUIREMENTS.md#to-verify-owner).

## Input and isolation

Apple M5 Max, macOS 26.5.2. The bundle was built from the model-constraint
changes committed as `f0877f9c`; its complete packaged checks and real
generation are recorded in [model constraints](model-constraints-2026-10-07.md).
The source was that run's accepted 30-frame LTX bridge over `cfr-bframes.mp4`,
revision `c25c2275-fc10-4f25-a197-ef411aa8d70e`.

The executed `deadpan-cli` SHA-256 was
`f9c572bf5a7d314d996619ebed6e4ae0aad9365ba543e24033e44a3ca8f5a2eb`.
Evidence is under `/tmp/deadpan-resume-20261006/real-portable-3`, with the
script at `/tmp/deadpan-resume-20261006/verify-real-portable.py`.

The script copied the accepted package through `project copy-portable`, copied
that result again, then removed the intermediate package. All subsequent
validation, rendering, picture comparison and cleanup used an empty `HOME`,
scrubbed environment and a sandbox that denied outbound IP and reads of:

- the original source package;
- the removed intermediate package;
- the bundle's AI runtime;
- the isolated home containing the installed model packs;
- the development model cache.

Before running the workflow, separate probes confirmed `EPERM` when reading
the existing source database, runtime manifest and model-selection file, and
when opening an outbound IP connection. `denial-checks.json` retains those
results. No inference or native window was started.

## Results

| Check | Result |
| --- | --- |
| Two verified portable copies | Passed, 0.210 and 0.204 seconds |
| Copied document and complete history validation | Passed; document exactly matched the source |
| Offline render | Verified and published, 2.257 seconds |
| Decoded preview/export picture comparison | Passed at frames 59, 60, 75, 89 and 90, covering both seams and generated interiors |
| Explicit cleanup | Removed only two released `render_candidates` objects, 159,401 bytes; accepted document unchanged |
| Render after cleanup | Verified and published, 2.128 seconds |
| Source document after the run | Exactly unchanged |

`summary.json` retains timings, hashes and removed-object identities. Render
performed its full emitted-file verification; the additional `verify-export`
comparison checked pictures with `--no-audio`. The existing
`generated_joins` speech-preservation tests cover the unchanged audio bus.

The full prior gate passed 4,740 workspace tests and 1,027 UI-harness tests.
The follow-up workspace run with the colour conversion also passed all 4,744
tests, including `offline_portable` (52.619 seconds), both real-media
conditioning cases and the generated-neighbour case. Storage tests cover
held readers, history/register/checkpoint pins, discarded and stale variants,
missing linked Originals, media removed during copying, complete copies and
cleanup restricted to the previewed set.

## Retained test failures

The first harness used an unsupported `project validate --full` flag; full
validation is the default. The command refused before validation or rendering.
The second workflow passed, but a separate denial probe found that the model
home rule used `/tmp` while Seatbelt matched `/private/tmp`. Runtime and source
reads were denied, but that model-home rule was ineffective. The final run
canonicalized every denied path and required all denial probes to pass before
the workflow. Both earlier attempts remain in the scratch evidence.
