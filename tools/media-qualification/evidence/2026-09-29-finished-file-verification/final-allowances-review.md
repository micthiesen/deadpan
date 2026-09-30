# Final decoder allowances and qualification harness review

Source-only review of the H.264 allocation bound, H.264 profile observations, AAC allocation bound, `qualify_encoded_verification.rs`, and the changes to `qualify_project_picture/encoded.rs`. No builds, tests, formatting, native decoder execution, or commits were performed by this reviewer.

## Decoder allowances

No actionable issue was found in the three allowances:

- **H.264 coded raster:** `inspect.rs` rounds each admitted visible dimension upward to a 16-pixel macroblock boundary when calculating the decoder allocation budget. The native encoder contract already bounds both dimensions to 8192; the rounded dimensions remain within the source adapter's hard bounds. The MP4 sample description, displayed dimensions, decoded visible raster, and I420 byte count still have to equal the original output contract. This allows codec padding without changing visible geometry admission.
- **Unobserved codec-parameter profile:** picture validation accepts only `-99` or `100` in FFmpeg's container `codecpar` observation. It still requires actual decoder profile `100`, while container inspection independently requires avcC profile `100`. Unknown remains an observation, and no profile is inferred from it. Explicit conflicting profiles still fail.
- **AAC allocation:** the decoder allocation ceiling is now 2048 samples per channel, with two channels and 48 kHz still fixed. The actual decoded metadata must still report exactly 1024 samples, and the owned PCM copy must contain exactly 2048 interleaved stereo values. Packet counts, priming, sample coordinates, terminal durations, and whole-file physical counts remain exact. I independently read the pinned source at `/tmp/deadpan-ui-ffmpeg/ffmpeg-8.0.3/libavcodec/aac/aacdec.c`: lines 202-203 set `ac->frame->nb_samples = 2048` before `ff_get_buffer`; output paths later assign the actual `samples` count. The allowance bounds a real temporary allocation and does not widen returned AAC block admission.

## Harness findings

### P2 resolved: Retain newly rejected encoded files before verification

`qualify_project_picture/encoded.rs`, `run_case`: the new `verify(...)?` precedes the first MP4 copy and precedes recording the case manifest and verification progress. On rejection the error retains the anonymous candidate temporarily, but outer error propagation eventually drops it. The qualification run therefore loses the very encoded bytes needed to investigate or reproduce the new verifier failure.

Retain the unverified candidate and its manifest before invoking verification, and persist verification status/progress/error alongside them. Keep successful verification and independent content qualification as separate status claims. Retaining a private qualification artifact does not grant publication authority.

**Source disposition:** `run_case` now copies and synchronizes the MP4 before verification, writes an exclusive synchronized manifest sidecar, and inserts a pending case into the outer report. Verification failure records failed status, its diagnostic, and collected progress before returning. Successful verification records passed status, the report, and progress before direct reference preparation begins. Candidate bytes and claims survive rejection, and a later reference-preparation failure preserves the observed verifier result. No issue remains from this finding.

### P2 resolved: Preserve standalone harness failure and teardown evidence, and reject an empty run

`qualify_encoded_verification.rs`: an empty `encoded.cases` array reaches `status = passed` without starting a worker. The event loop records only messages, discarding supervisor Fault/Exited observations and the captured stderr tail. A supervisor failure can therefore leave `failure = null` and `report = null`, followed by an unhelpful outer error, while the saved status remains `running`.

Require a nonempty case list, record Fault and Exited including cancellation escalation, retain bounded supervisor logs, and set an explicit failed status before saving a failed case. These changes improve retained evidence; the existing supervisor already withholds Completed until clean teardown, so the event omission alone does not admit an unclean successful candidate.

**Source disposition:** the harness now rejects missing or empty cases, records supervisor faults, requires an observed successful non-escalated exit, retains bounded stderr and discarded-byte counts, and persists failed status before returning a worker failure. Poll errors also become explicit failures; they cannot produce a passed report. It no longer accepts an ambient dynamic-loader override. No actionable admission or teardown issue remains in this change. This is source inspection only; the revised evidence capture has not been exercised by this reviewer.

## Observed qualification scope

I read `/tmp/deadpan-export-verify-mkzjo3zd/retained-aac-report.json`. It records seven cases (`structural`, `nonzero`, `software-two`, `odd`, `marker`, `generated`, and `after-cancel`), each with `failure: null`, and a final `passed` status. Reported counts total 304 pictures and 390,695 authored audio samples. These are existing-run report observations, not a test run performed by this reviewer.

The standalone harness exercises the production verification child over retained files. It does not exercise `verify()` candidate staging/retry ownership, new encoding, destination publication, or content comparison against original picture/PCM references. The integrated qualification path calls the production host and preserves separate pending independent content qualification. Those distinctions should remain in the final evidence writeup. Full product export and all broader release claims remain outside this review.
