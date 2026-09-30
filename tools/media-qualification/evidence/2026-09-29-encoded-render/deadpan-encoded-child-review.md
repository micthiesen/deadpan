# Encoded child review

Read-only review of `crates/deadpan-cli/src/encoded_render/worker.rs`, its focused tests, the shared control/output helpers, and the relevant picture, offline-audio and native-encoder entry points. No builds or tests were run.

## Findings

No actionable correctness finding in the reviewed scope.

## Checked boundaries

- `prepare` reconstructs the native contract, admits the requested limits, captures the exact picture revision/range, and compares every picture-contract field. It separately captures audio at the same revision and range, compares both absolute sample endpoints and the exact sample count, then hashes both complete immutable documents against the host binding before allocating the GPU or output file.
- Picture inputs preserve the captured output ordinal, PTS, duration, raster, pixel policy and byte length. A single completed frame remains owned through synchronous `push_picture` and is dropped before the next input. The native input scheduler enforces the same next-input order and poisons rejected sessions.
- Audio reads start at `B(range.start) + first_sample` and cannot leave `[B(start), B(end))`. The short final AAC input retains its exact count. Planar conversion checks length and finiteness and applies no gain, padding or event alignment.
- The request deadline is passed unchanged through audio capture/reads, GPU preparation, encoder calls and final hashing. The control reader expires against that same child deadline and sets the cancellation flag. Synchronous SQLite, GPU, decoder or filesystem operations remain cooperative; the host's separately retained caller deadline and supervised process teardown provide the outer enforcement. This remains a documented limitation rather than a preemptive native guarantee.
- The shared control pump drains available bytes before honoring local shutdown. Queued valid cancellation overrides a just-produced movie; wrong identity/token, a second Prepare, malformed input, EOF, and a partial buffered frame fail. The child tests cover queued wrong token/version, partial header and EOF. A control message that races after the final WouldBlock boundary is handled by the host's own cancellation/terminal policy.
- The output is an exclusive 0600 regular descriptor created beneath the pinned output directory with no-follow and close-on-exec flags. Native encoding and final SHA-256 use that owned descriptor. The child checks its exact final length before and after bounded hashing, and the parent independently snapshots the declared path after teardown.
- The child validates the completed manifest against its exact request limits before sending it. Those native fields remain claims. No independent emitted-file verification or publication authority is established by completion.

## Verification still owned by parent

Compile/format/Clippy, focused protocol and child tests, real supervised admission/cancellation tests, and actual native encode/decode qualification. The source review does not establish an encoded export success claim.
