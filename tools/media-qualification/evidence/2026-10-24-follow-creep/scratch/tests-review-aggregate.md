Review result: no surviving finding.

The independent cap/union coverage is sound. `ANIMATED_CHILDREN` contributes 99,998 records; the raw document fixture adds two fixed Follow records to reach exactly 100,000, is parsed through `ProjectDocument::from_json`, and asserts the typed inventory. Patch assertions cover the exact legal after inventory and successful application/inverse, then an exact-cap patch whose retained unchanged fixed Follow makes the applied union 100,001, followed by a distinct 100,001-record patch inventory rejection. Each failed apply is followed by byte equality of the source document.

The Hold capture refusal is scoped accurately: it asserts the audio-binding complexity limit and explicitly avoids claiming framing admission or preallocation behavior. The separate raw slice case parses and roundtrips an exact-cap standalone slice with the physical Hold/binding payload, then rejects one-over through `CapturedEditSlice::from_json`; `validate_capture` is expected to fail because this payload is deliberately not claimed as an authentic capture of the small source.

I verified these claims against `follow_aggregate.rs` and the current `DocumentPatch::apply`, `CapturedEditSlice::from_json`, and framing inventory validation paths. No code changes, builds, tests, Cargo commands, runtime actions, or Git writes were performed.
