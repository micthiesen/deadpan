# Store authority additions: source-only handoff

Only owned source changed: `source/crates/deadpan-store/tests/compound_preparation.rs`. Existing parent changes were retained. The earlier Core integration file was not touched in this follow-up.

Three new tests:

- `one_leaf_replay_admitted_reverse_audio_rechecks_stored_qualification` decodes and admits the existing offset-bframes.mp4 through the public managed Original/source registration APIs. It builds an eligible semantic pause with admitted Reverse samples, previews the exact request successfully, then removes only the stored qualification row after witness creation. Candidate and None must return exactly `SourceRegistration("Hold audio source qualification is missing")`, call host preparation zero times, and preserve every database table cell and the authored snapshot after the deliberate receipt removal.
- `one_leaf_replay_changed_corrections_invalidate_admitted_transcript_guard` saves synthetic transcript annotations on that actually admitted Original. The captured absent-corrections guard passes an exact preview. A genuine `change_analysis_corrections` Apply changes the first recognized word. The fresh stored guard passes the same request, while candidate and None must reject the captured guard with exactly `Integrity("transcript selection analysis changed")`, callback zero, unchanged authored snapshot and every table cell.
- `one_leaf_replay_historical_allocations_refuse_before_preparation` advances the head from initial to later, creates an eligible fresh plan, then separately reuses historical initial for its outer or leaf allocation. Both paths must return exactly `RevisionReused("initial")` before callback and preserve the snapshot and all tables. This targets retained history rather than current-head equality.

The admitted fixture helper follows `tests/transcript_selection_guard.rs::ready`; the sample endpoints follow `tests/source_registration/hold_audio.rs::source`. No receipt, qualification proof or new ceiling is fabricated. The existing silence-only planner helper delegates to a private helper taking a HoldAudio value, leaving its former behavior unchanged.

Static source review followed the actual visitor ordering: Store allocation reservation precedes replay; every leaf still visits validate_transition; Reverse InsertTime invokes validate_hold_audio_source; guard comparison uses the current stored analysis. No production source changed.

Uncompiled, unformatted and unexecuted by this worker. No Cargo, tests, formatter, build, runtime, benchmarks or Git commands ran. Parent owns all verification. First focused run must establish the existing decoded fixture's semantic pause eligibility and successful previews, then the exact asserted refusal messages. No authentic-fixture blocker was found in source review.

Snapshots retained:

- `compound_preparation.authority-before.rs.txt`: SHA256 `a3adfd8b29d41e9c120bd3b75d9cd8984febfab44ede7f2e02a122bf5a381708`
- `compound_preparation.authority-after.rs.txt`: SHA256 `1706eaf64a98da3634570e24d44c0fa0b0aed9e093657f4d70ad3c5c98400884`

The before snapshot includes the parent's already-written four replay Store tests; this follow-up patch is only the authority extension. Parent retains the baseline-wide map and final complete patch.
