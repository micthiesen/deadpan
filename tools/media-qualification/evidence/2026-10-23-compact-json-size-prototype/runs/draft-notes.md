# Uncompiled compact JSON size draft

Status: uncompiled and unformatted. No repository files were edited, and no Cargo, formatter, Git or app/runtime command was run. The generator reads each current source immediately before building its proposed text; source-hashes.json pins original and proposed bytes for all five files.

## Scope

- Core document.rs: new compact_json_size API, private low-limit helper, and a checked counting writer. It uses the same serde_json compact serializer, limit/error precedence and messages as to_compact_json. Existing byte-producing serializers and BoundedJson are untouched.
- Store compound.rs: count only entry/final documents and uncaptured leaf after-states. Captured stages retain the exact existing String. Serialization still precedes processed-byte charge, step bound, checkpoint charge and StepRecord insertion.
- Store lib.rs: extract check_document_size_bytes so the existing explicit Store guard and message remain at the same final-state boundary. Existing string callers delegate their exact len to it.
- Two Core writer/API tests cover exact escaped/UTF-8 compact lengths, low-limit parity, chunked writes and overflow/sticky-limit behavior. One existing binding-fixture test covers nested resume/phase/reanchor wire size and roundtrip. One Store test compares captured/uncaptured staged checkpoints against a fully materializing reference, then checks actual persisted bytes, Undo/Redo and reopening.

The new size API does not validate the document, cache a size, change any wire vocabulary, or retain a validation proof. Existing serializers can allocate temporary per-field values; only the complete output buffer is eliminated. Standalone context codecs 1-12 and sound-clock serializers are not edited.

## Draft files

Each source has a per-file .patch plus original/proposed .rs.txt copies. instrumentation-free-size-count.patch combines those exact changes. build_draft.py requires each anchor once; rerunning it would adopt the source then current, so retain this manifest before any regeneration.

## Review and checks still owed

No compilation, formatting, unit test or performance result is claimed. Parent must recheck original hashes before integration, format the draft, and run focused Core serialization/binding and Store checkpoint tests, the existing Core work/capture bounds, standalone fixed-byte codecs and appropriate app/history regressions. The binding test reuses a genuinely validated nested state but does not claim every possible nondefault template shape is individually covered.

MAX_DOCUMENT_JSON_BYTES at its public 64 MiB threshold is exercised through equivalent small private limits here; full-size fixtures and Store processed/checkpoint limit cases remain existing-suite obligations. No synthetic generic serializer helper was introduced solely to force InvalidJson, so the existing non-limit error mapping is preserved directly by code rather than a new injected serializer failure test.

The Store test's exact three-stage fixture expects repeat-stage to be captured and cut-stage/paste-stage not captured. This matches the existing repeat_cut_paste fixture, but remains to be verified by execution. If a test assumption or borrow/type error occurs, retain it and correct the fixture without weakening authority, byte charges or scope.

This draft is a separate experiment after the active leaf-result reuse qualification. Do not apply it over that candidate while its source guards are running. No latency or memory improvement has been measured.
