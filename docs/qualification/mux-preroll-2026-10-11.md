# Reordered-video proof with current AAC, 2026-10-11

The private video-duration proof now budgets both AAC priming packets required
by the current encoder. It derives the extra packet count from the existing
priming and frame-size constants. No packet, authored sample, timestamp,
audio edit or verification tolerance changes.

## Failure and deterministic regression

The YouTube network milestone's full workspace gate passed 5,681 tests and
failed `hdr_pq_and_hlg_main10_files_pass_complete_inspection` during native
finalization. `mux_duration::prove` still allowed
`video_frames + ceil(audio_samples/1024) + 1` total MP4 samples. The current
AAC contract requires two priming packets, so a correctly emitted file was
refused with `mux_duration_evidence: aggregate declared media samples exceed
admission bound` when the variable reordered-video correction path ran.

The original failing test deleted its temporary file during unwinding.
It now retains the pre-finalization file and captured contract on failure.
Ten standalone HDR qualification runs and nine isolated/concurrent executions
of the exact test did not reproduce that packet order; all outputs and command
results are retained. These reruns do not negate the original failure.

The deterministic fixture combines the real reordered HEVC stream retained
in the [original duration qualification](mux-duration-2026-10-08.md) with
current native AAC from the new qualification run. A diagnostic FFmpeg 9.0.1
stream copy assembles them without re-encoding. Independent pinned FFprobe
8.0.3 verifies every packet's bytes, PTS, DTS and duration against its source:
46 video packets and 74 AAC packets, with AAC starting at -2048/-1024.
The remux repairs the old video's duration header; the fixture restores only
that known 47-tick `mdhd` defect. This is a documented reconstructed regression
input, not a claim that a fresh failing native file was captured. The production
FFmpeg dependency remains pinned at 8.0.3.

The 25,393-byte fixture SHA-256 is
`c47e32892cb99ae4b17356171b4cb471d691bee7c8a3a409b543ffdc4f81da02`.
`reordered_picture_proof_admits_both_current_aac_priming_packets` checks the
74-packet audio track and its 2048-sample edit, then invokes the production
finalizer. It fails against the previous implementation with the exact gate
error. With the correction, only byte 315 changes from duration 47 to 46;
every other byte remains identical, and a second finalization changes nothing.
The original one-priming-packet video regression remains available; final
output verification still rejects its retired AAC policy.

## Verification

All 64 native encoder tests and 172 encoded-output tests pass, including the
original failing HDR test and the deterministic old-code regression. The
repository gate completes in parts: the original 5,681 passing workspace
tests, the affected/new checks above, all 1,105 UI tests, both doctests and
strict workspace/UI Clippy. One passing UI test reports lingering output;
its isolated follow-up passes without that warning, whose cause is unestablished.
The fresh bundle passes relocation, media and AI runtime smoke tests and
tamper negatives. Final packaged checks are shared with the
[network milestone](youtube-network-2026-10-11.md#completion-evidence).

The sole agent reviewed the complete change. This is a Rust inspection-bound
correction and diagnostic test retention; the native ABI, encoder and decoder
implementations are unchanged. No native window or other agent was used.

[Evidence](../../tools/media-qualification/evidence/2026-10-11-mux-preroll/README.md)
retains the fixture, exact packet identities, failed old-code regression,
passing checks, source/executable hashes and capture attempts. Scratch files
remain under `/tmp/deadpan-mux-preroll-20261011`.
