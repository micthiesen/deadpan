# Native and public Render evidence

This record qualifies the current automatic SDR native action and closed-project
headless commands on the development host. See the
[qualification and limits](../../../../docs/qualification/public-render-2026-09-30.md).
No full-product, HDR, complete mastering or release-packaging claim is made.

## Results

| Check | Result |
| --- | --- |
| Workspace tests | 2,463 passed; no failed or ignored tests |
| Optional UI-feature tests | 318 passed; no failed or ignored tests |
| Strict workspace and UI Clippy, formatting | Passed |
| Native Metal startup/shutdown | Passed |
| Final Render visual replay | 71 checks plus the shortcut audit; eight inspected captures |
| Full release replay | 2,418 checks; no findings or failed/timed-out timing samples |
| Public workflow | Eight commands, including cancellation and checkpoint recovery |
| Independent full-file decode | Both public encodes; 768 complete picture planes and all authored audio |
| Native save sheet and export | Passed within the limitations in `native-ui-report.json` |

The ordinary release replay explicitly skips the separate accepted-generated
picture fixture. Native filename entry needed an accessibility correction and
pointer focus; fully keyboard-only save acceptance, physical input, non-US
layouts, IME, VoiceOver and physical display performance remain unqualified.

## Retained material

- `summary.json` records counts, command outcomes, skipped scope and remaining work.
- Command journals and logs retain both successful checks and earlier failures.
- `source-coverage.json` binds each check to its exact source inventory and lists
  changes between earlier observations and the final checked source. Source
  equality is not a substitute for behavioral verification.
- `replays/` retains compressed complete visual and release reports.
  `performance-summary.json` contains the timing distributions without samples.
- `screenshots.json` maps the eight final decision/result PNGs to named checkpoints.
- `native-files.tar.gz` retains synthetic project packages, SQLite backups,
  movies, publication reports, independent decoder output and canonical references.
  `native-files.json` identifies all 74 members. Every member was reopened and
  rehashed; live databases were copied through SQLite's backup API.
- `independent-full-file.json` records absolute-clock picture/audio comparisons.
- `review.txt` records the final code and image review. The scratch collection and
  qualification scripts are retained as text.
- `manifest.json` hashes every other file in this directory, including this README.

The copied QA application, compiled helpers, mass intermediate PNGs and transient
database sidecars are excluded. Their relevant identities and observations stay
in the reports. `retained-files.json` records the collector's original inputs;
the manifest additionally covers this parent-written README.
