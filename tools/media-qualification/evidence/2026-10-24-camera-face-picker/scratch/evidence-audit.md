# Camera face-picker evidence audit

Status: no_findings.

Read-only offline verification. No product or build command was executed.

Final ten source snapshots and scoped receipts match current source. First and final retained App/helper bytes are verified separately.
One additional Rust path has a proved single diagnostic-string amendment after the runs. Its corrected wording is not claimed to have been executed.
Raw results: 1,301 App unit tests passed, 11 existing ignored; 35 pre-polish Camera tests; four final face-service tests. Final native faces/targets/Zoom have 66/41/213 checks plus one Kestrel check each, with their warnings and skips preserved.
Nine real CLI commands passed. The actual 128,183-byte movie verifies all 120 pictures and [0,192192) audio samples at 30000/1001 with zero offsets/gaps/failures. Portable authored document and 13 revision rows match, with seven actual history rows.
The twelve-face positive data is scripted. The real counter-fixture Vision zero-face result, earlier stale-doc test failure and pre-polish visual issue remain explicit. Shared picture/DSP references do not establish independent DSP, listening, tracking accuracy, physical input or latency qualification.

See evidence-audit.json for exact pins, ignored names, warnings, skips and proof limits.

Earlier worker compilation retained two E0308 and two E0624 errors, followed by the successful corrected check. Successful debug focused-test logs retain the linker __eh_frame size warning. The final strict command completed with exit 0.
