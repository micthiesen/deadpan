# Proxy encoder faults and retry-test isolation, 2026-10-11

The intermittent packet-order failure was reproduced at the native encoder
boundary. Production still rejects it and retries once in a fresh worker after
confirmed teardown. No packet check, retry limit, timing requirement or encoder
setting was relaxed. This work corrects a lifecycle test's dependence on a
second hardware operation succeeding after its intentionally failed first run.

## Observations

The [thumbnail milestone](youtube-thumbnail-2026-10-11.md) retained a full-gate
failure in `a_stalled_worker_is_torn_down_and_retried_once_in_a_fresh_output`:
packet 20 carried PTS 21021/DTS 20020 where PTS 20020 was expected. An isolated
rerun passed. Similar failures are retained in the earlier
[proxy qualification](proxy-2026-10-05.md#failures-and-robustness).

The production encoder already requests `gop_size=1` and `max_b_frames=0`.
Pinned FFmpeg sets `AllowFrameReordering` to false for this configuration.
Apple documents [that property](https://developer.apple.com/documentation/videotoolbox/kvtcompressionpropertykey_allowframereordering)
as disabling encoder reordering, and documents
[compression output handlers](https://developer.apple.com/documentation/videotoolbox/vtcompressionoutputhandler)
as delivering decode order. The failure is not evidence that the app omitted
the requested setting. It does not identify whether FFmpeg or the underlying
service caused the violation.

A diagnostic copy of `converter.c` replaced only packet admission/muxing with
packet tracing and independent single-threaded H.264 decoding. Encoder setup,
RGBA-to-NV12 conversion and frame submission stayed the production code. The
copy never produces a valid proxy or changes shipped behavior. All runs used
the qualified FFmpeg 8.0.3 libraries on the M5 Max/128 GiB, macOS 26.5.2.

| Probe | Runs | Completed | Result |
| --- | ---: | ---: | --- |
| Sequential 256×144 binary-labelled pictures | 200 | 200 | 24,000 decoded picture IDs match their PTS. |
| Eight concurrent 320×180 binary-labelled streams | 400 | 399 | Completed runs preserve all IDs and PTS; trial 148 exceeds the 25 s process limit. |
| Eight concurrent encodes of the actual CFR fixture | 400 | 399 | Trial 323 swaps two packet PTS values, then stops making progress. |
| CFR follow-up, flushing raw packets and stopping on an order violation | 800 | 799 | No further order violation; trial 41 exceeds the process limit after 105 packets. |

The decisive trace from trial 323 is:

| Arrival ordinal | Expected PTS | Actual PTS | Actual DTS | Key picture |
| ---: | ---: | ---: | ---: | --- |
| 4 | 4004 | 4004 | 4004 | yes |
| 5 | 5005 | 6006 | 5005 | yes |
| 6 | 6006 | 5005 | 6006 | yes |

The diagnostic decoder emitted the same two PTS values in that arrival order.
Frame submission subsequently stopped inside input 22; the diagnostic process
hit its 25 s limit. No stack sample was taken, so this is not claimed as proof
of a particular blocked VideoToolbox function. The earlier qualification has
separate sampled-stack evidence for service hangs.

Raw packet writes in that first probe were buffered and the timeout left its
packet file empty. Therefore the pictures inside those two specific packets
were not independently matched to Original frame IDs. The follow-up flushes
each packet, but did not reproduce another swap. These limits are retained;
this experiment does not justify sorting or relabelling questionable packets.
The existing early refusal avoids continuing a session after its first
contract violation. Three timeouts among 1,800 diagnostic runs are observations,
not an estimated production failure rate. Eight concurrent sessions deliberately
exceed the app's one-proxy-session policy.

The actual-CFR diagnostic input was decoded to RGBA with the development
FFmpeg 9.0.1 executable. The qualified prefix contains libraries and ffprobe,
not an ffmpeg executable; the metadata records the PATH-selected executable
and its hash. This is distinct from the recorded regression fixture below,
which the real worker decodes through the production source adapter.

## Deterministic lifecycle regression

The failing test deliberately stalls its first worker, using up the one retry.
Its second run previously invoked the actual encoder, so any unrelated service
fault failed the lifecycle assertion even when teardown, fresh staging and
retry limits were correct.

The test now replays one real worker's complete output and success reply at that
boundary. `deadpan-media-worker proxy` produced the retained 66,929-byte MP4
from the owned `cfr-bframes.mp4`: 120 intra pictures at exact `n * 1001` PTS,
time base `1/30000`, 320×180, BT.709, quality 60. Its request, reply, worker hash
and source/output hashes are retained with the regeneration script.

The test still checks exactly two launches, no surviving stalled process,
removal of the first staging file, retention of only the second, complete
byte equality with the fixture, report length, and independent `verify_proxy`
timing/pixel validation against the Original. The unrelated encoder slot is
no longer acquired by this test. Other proxy tests continue actual encoding,
range assembly, interpretation checks, cancellation and service-failure retries.

## Verification and scope

Formatting and strict media-worker Clippy pass. All seventeen proxy real-media
tests pass. Two passing tests report lingering output; both pass their isolated
follow-up without that warning. The lifecycle test passes twenty consecutive
runs. Commands and results are retained in the evidence.
No shipped implementation or native ABI changed. The full workspace gate,
sanitizers and bundle were not repeated for this test/fixture-only change.
The thumbnail milestone's full-gate failure remains historical evidence; this
does not relabel it as a passing run or claim the encoder service is fixed.

[Evidence](../../tools/media-qualification/evidence/2026-10-11-proxy-order/README.md)
retains all probe summaries, the three failure traces, diagnostic sources and
hashes, real fixture generation, and regression logs. Complete successful probe
captures and large binaries remain in `/tmp/deadpan-proxy-order-20261011`.
