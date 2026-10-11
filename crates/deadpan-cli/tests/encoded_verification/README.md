# Finished-file verification fixtures

The three current MP4 files were generated on 2026-10-10 by the real native
encoder's `qualify` example using ABI 3 and explicit SDR policy v2. That encoder
requests frame-QP 24 and a three-times-target rate ceiling, and feeds a silent AAC warm-up block at
negative PTS and retains an exact 2,048-sample priming edit. The input recipe is
the example's synthetic picture markers and stereo impulses. Raw producer
reports are retained as `fixtures/*.qualification.json`; their actual reports,
movie hashes and lengths are bound into the adjacent test manifests.

`fixtures/provenance.json` records each exact command, the executed binary hash,
source hashes, file sizes and movie hashes. The contracts and document hashes
remain labels for the transport tests' background projects. They do not claim
that the synthetic pictures or audio came from those projects.

| Fixture | Pictures | Authored audio samples | Relevant boundary |
| --- | ---: | ---: | --- |
| `nonzero` | 1 | 1,601 | Range [1, 2) at 30000/1001 fps; nonzero absolute sample origin |
| `hardware-gops` | 43 | 68,869 | Hardware H.264 across multiple GOPs; range [20, 63) at 30000/1001 fps |
| `marker` | 120 | 96,000 | 60 fps picture/audio marker fixture |

The unchanged ABI-2 `software-two` movie, manifest and producer report remain
here for the historical full-decoder unit test. That test checks all 43 frames,
fresh GOP decoding, B-frame ordering and exact AAC clocks, while proving the
old manifest cannot enter live admission. Current OS software H.264 lacks the
QP property and is refused by the quality policy; its older movie is not
relabeled as evidence of the new controls.

The previous ABI-2 `nonzero` and `marker` files are archived at `fdef263d`.
The previous ABI 1 files and manifests remain archived at commit
`63d81ca4713fc486ced8a408ea471773f959a576`, at these same paths. Their original
provenance is preserved in `fixtures/provenance.json`. The earlier producer and
broader numerical/content comparisons are documented in
[encoded-render qualification](../../../../docs/qualification/encoded-render-2026-09-29.md).
The current three movies total 35,062 bytes:

```text
3a97e1d2aa6d188a94f85e4acddbc6185d3597e6ed5ee1836c464dcdfb552c4b  fixtures/nonzero.mp4
b294934b3de38ba605a6a04c8fa30bc9eed9572e17e1959ec12abb4992a77b27  fixtures/hardware-gops.mp4
184a863cee8f0d0ea8779c2a9d837b6c64532077fff03fcd7ae60f9764a8bf74  fixtures/marker.mp4
```

## What the tests establish

The integration test creates a small background-only project with matching project/revision IDs, canvas, rate, duration, and selected range. The Python encoder transport asserts equality of the complete requested contract, replays a retained movie, and rebinds only the document hash and actual movie hash/length. This exercises the production candidate admission path without invoking an encoder or GPU during the test. It deliberately does not establish that replayed pixels or audio came from the background project.

The real `CARGO_BIN_EXE_deadpan-cli` verifier then inspects and decodes the private candidate. Tests check all three files, exact report bindings and retained bytes, cancellation with retry, and rejection of altered geometry, color, edit timing, duration, sync claims, NAL lengths/types, and IDR slice payloads after recomputing the movie hash. Project snapshots must remain unchanged.

The Python verifier branch emits protocol claims only. Its malformed/stale responses, progress regressions, changed hashes, inconsistent reports, messages after completion, and failed process exits test the host boundary. It never decodes a file and is never selected by the application. A real native retry after those faults checks candidate retention independently.

These are decoder and host integration fixtures. They do not confer content qualification, runtime release qualification, or destination publication authority. The repository's separate actual encoder, content/sync, and sanitizer qualification remains required. No test writes an exported destination.
