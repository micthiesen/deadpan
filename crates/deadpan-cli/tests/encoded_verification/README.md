# Finished-file verification fixtures

These three MP4 files were generated on 2026-10-08 by the real native encoder's
`qualify` example using ABI 2. That encoder feeds a silent AAC warm-up block at
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
| `software-two` | 43 | 68,869 | OS software H.264 with B pictures; range [20, 63) at 30000/1001 fps |
| `marker` | 120 | 96,000 | 60 fps picture/audio marker fixture |

The previous ABI 1 files and manifests remain archived at commit
`63d81ca4713fc486ced8a408ea471773f959a576`, at these same paths. Their original
provenance is preserved in `fixtures/provenance.json`. The earlier producer and
broader numerical/content comparisons are documented in
[encoded-render qualification](../../../../docs/qualification/encoded-render-2026-09-29.md).
The current three movies total 40,561 bytes:

```text
d96c4316ce39cb02d0334c422e3899b69f101c233fd25d017b5e987b09ae2bab  fixtures/nonzero.mp4
24f4aa1c0a67f00e30aaa557a51a89c89c26ffdd5bae40ba4d4b25fbf65d49ec  fixtures/software-two.mp4
27150b80303fbf93c051516691c94a213aa89897e7093ec933bd6232301a2e9b  fixtures/marker.mp4
```

## What the tests establish

The integration test creates a small background-only project with matching project/revision IDs, canvas, rate, duration, and selected range. The Python encoder transport asserts equality of the complete requested contract, replays a retained movie, and rebinds only the document hash and actual movie hash/length. This exercises the production candidate admission path without invoking an encoder or GPU during the test. It deliberately does not establish that replayed pixels or audio came from the background project.

The real `CARGO_BIN_EXE_deadpan-cli` verifier then inspects and decodes the private candidate. Tests check all three files, exact report bindings and retained bytes, cancellation with retry, and rejection of altered geometry, color, edit timing, duration, sync claims, NAL lengths/types, and IDR slice payloads after recomputing the movie hash. Project snapshots must remain unchanged.

The Python verifier branch emits protocol claims only. Its malformed/stale responses, progress regressions, changed hashes, inconsistent reports, messages after completion, and failed process exits test the host boundary. It never decodes a file and is never selected by the application. A real native retry after those faults checks candidate retention independently.

These are decoder and host integration fixtures. They do not confer content qualification, runtime release qualification, or destination publication authority. The repository's separate actual encoder, content/sync, and sanitizer qualification remains required. No test writes an exported destination.
