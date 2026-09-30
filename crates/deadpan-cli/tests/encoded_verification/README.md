# Finished-file verification fixtures

These three MP4 files are unchanged outputs retained from the native encoded-render qualification on 2026-09-29. Their original manifests are beside them. `fixtures/provenance.json` records the original paths, source-report SHA-256, original document hashes, file sizes, and movie SHA-256 values.

| Fixture | Pictures | Authored audio samples | Relevant boundary |
| --- | ---: | ---: | --- |
| `nonzero` | 1 | 1,601 | Range [1, 2) at 30000/1001 fps; nonzero absolute sample origin |
| `software-two` | 43 | 68,869 | OS software H.264 with B pictures; range [20, 63) at 30000/1001 fps |
| `marker` | 120 | 96,000 | 60 fps picture/audio marker fixture |

The original producer and broader numerical/content comparisons are documented in [encoded-render qualification](../../../../docs/qualification/encoded-render-2026-09-29.md). These files total 64,871 bytes:

```text
3ed39453e601785efa9341714948a51b0f61d4a0c52568b2b2ceb9960281abaf  fixtures/nonzero.mp4
b1e886e6b946936cbe23a24b9eb0a0e18f26b339dd9f202ec0dc5c7bebf9a72e  fixtures/software-two.mp4
4658f6dd1a701d2ee7fb54630fe9ecbd41708881e7d4f21b865ad7ca1ddd8c86  fixtures/marker.mp4
```

## What the tests establish

The integration test creates a small background-only project with matching project/revision IDs, canvas, rate, duration, and selected range. The Python encoder transport asserts equality of the complete requested contract, replays a retained movie, and rebinds only the document hash and actual movie hash/length. This exercises the production candidate admission path without invoking an encoder or GPU. It deliberately does not establish that replayed pixels or audio came from the background project.

The real `CARGO_BIN_EXE_deadpan-cli` verifier then inspects and decodes the private candidate. Tests check all three files, exact report bindings and retained bytes, cancellation with retry, and rejection of altered geometry, color, edit timing, duration, sync claims, NAL lengths/types, and IDR slice payloads after recomputing the movie hash. Project snapshots must remain unchanged.

The Python verifier branch emits protocol claims only. Its malformed/stale responses, progress regressions, changed hashes, inconsistent reports, messages after completion, and failed process exits test the host boundary. It never decodes a file and is never selected by the application. A real native retry after those faults checks candidate retention independently.

These are decoder and host integration fixtures. They do not confer content qualification, runtime release qualification, or destination publication authority. The repository's separate actual encoder, content/sync, and sanitizer qualification remains required. No test writes an exported destination.
