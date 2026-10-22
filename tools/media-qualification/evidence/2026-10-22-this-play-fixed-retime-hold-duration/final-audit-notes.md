# Final source, replay, and release-media audit

## Result

The final-source snapshot, retained source archives, evidence inventory, recorded executable identities, actual MP4s, and package files all hash-verify. The raw release export results substantiate the qualification's output totals and claims. The qualification, requirements row, and handoff preserve the narrow fixture scope and do not mark any new requirement complete. I found no false claim that the full This-play duration requirement has been closed.

The evidence inventory contains 185 entries totaling 14,112,676 bytes; every listed file has the recorded size and SHA-256. The inventory file itself is the one expected unlisted file. The 1,938-file baseline archive and all nine final/intermediate source archives match their per-file identity maps. All 32 final Rust paths match both the final source archive identities and the current checkout. The compressed executed verifier script decompresses to the exact retained executed script.

The five recorded executable identities match their retained binaries. The release deadpan-app SHA-256 is 0b04f601d7cef69e51624ad9181fcaabd4017495b654ff901a0351369d4e4b25. The four retained MP4 byte counts and hashes match encoded-identities.json. The four current temporary project packages also match every recorded file size and SHA-256.

The encoded reports support 160 verified pictures, 256,256 presented audio samples, four complete audio windows, offset zero, no flags, uncovered samples, low-SNR blocks, or failures, a minimum block SNR of 30.386750732875164 dB, 198 observable neighbor comparisons, and 114 explicitly unobservable comparisons. The qualification rounds the minimum SNR to 30.386751 dB. The four layout results are unit-rate growth/shrink and affine growth/shrink; each is a complete result with no failures.

The native replay claims match the retained reports: visual mode has 56 checks and 523 frames; performance mode has 56 checks and 678 frames; Kestrel checks pass. Both runs retain 26 second-retry frames and three runs of consecutive retries with distinct causes. These remain warnings, not test failures, and the qualification says so. The metrics match the report: state p99 1.828125 ms over 151 samples, commit max/p99 78.706042 ms over six, and picture-complete max/p99 8.833667 ms over 41. The report records 1280x820 as the initial/final viewport; the controls scenario resizes to 960x640 and restores 1280x820. The qualification accurately describes the inspected captures as 960x640 and does not claim every performance sample used a fixed 1280x820 viewport.

Historical failures are reported accurately: first core/app run 27/31 passed with four invalid fixture assertions; first all-package run 38/39 passed with the store final reopen revision assertion as the sole failure; corrected focused suite 39/39; an earlier broad run 3,495/3,496 followed by corrected 3,496/3,496; final changed-package normal and UI app runs 204/204 and 206/206, with strict normal/UI lint passing. The old-code witness failed only its final native availability assertion, while preceding structural tests passed; it was silent and did not establish media behavior.

The scope statement is consistent across the qualification and handoff: one explicit Play from a three-play Repeat, with one unclocked fixed Edit Retime beneath it. The tests do not prove nested/multiple ancestor combinations, mixed or selected plays, other Retime clocks/placements, every beat composition, codec, or the full editor workflow. Physical input, VoiceOver speech, and listening remain owner checks. The requirement stays partial; no new gate is marked complete.

## Findings

1. Minor, evidence durability: the four owner-step package paths in encoded-identities.json currently exist and every file matches its recorded identity, but they are under the OS temporary directory (/private/var/folders/.../T/.tmptWprYY), outside the collected evidence inventory. The copied MP4s and reports are durable in the repository evidence directory, but the owner workflow's starting .deadpan package may disappear when temporary files are cleaned. Retain a package under the evidence directory or update the owner step to reconstruct/copy it from a durable retained package before relying on the workflow.

2. Minor, wording: docs/spec/AGENT_HANDOFF.md says “Final normal/UI app runs pass 204/206,” which can read as 204 of 206 passing. The raw results and qualification establish 204/204 normal and 206/206 UI. Say “204 normal app tests and 206 UI-feature tests pass” to preserve the exact counts.

## Retained limitations

- No requirement or gate is newly complete. DP-06 and DP-07 remain partial.
- Acoustic quality, actual listening, physical key delivery, and VoiceOver speech were not established.
- The exported output layouts and retained fixtures establish only the listed unit/affine growth/shrink cases, not broad codec or editor qualification.
- Performance measurements are small scenario samples from the named machine and uncontrolled power/cache state; GPU readback and physical display behavior were not measured.

## Root resolutions after the final audit

The handoff and requirement summary now say 204 normal app tests and 206 UI-feature
tests pass, eliminating the ambiguous fraction. The exact nine-file unit-grow
package is retained in this-play-fixed-unit-grow.deadpan.tar.gz (75029 bytes, SHA-256
01fc5a0ca30502267e1bdeca745951e4e4f14f4e8cdd92317a010a21492bd450). Each original file matched encoded-identities.json before
archiving; an independent extraction matched every per-file digest. The extracted
copy's full project validation reports valid=true, 43 frames and 23 nodes.
The hashed release CLI's read-only doctor reports its one managed qualified
Original with 120 indexed pictures and a 43-frame plan. Its broader partial and
unimplemented capability lists remain in the retained report; this is a portable
fixture check, not product completion. Owner instructions now link the archive.
