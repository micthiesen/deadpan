# Oracle module correction

The exact existing Core unit oracle is:

audio_reference::scoped_capture_property::full_layout_reuse_ten_thousand_hold_interior_pause_matches_reference

Its source is crates/deadpan-core/src/audio_reference.rs:3505, inside the private scoped_capture_property module beginning at line3198. The earlier audio_reference::tests:: module label in the design/handoff was incorrect. This correction does not change any source or assertion.

Root retained the actual v1 E0659 test-helper compilation failure and v2 zero-test exact-filter refusal in their immutable attempt roots. The v2 binary's retained --list discovery records this exact module path in v3 previous-attempts.json; v3 expected-tests.json now uses it. No product measurements occurred in either failed attempt. This note is a static provenance correction, not a claim that this reviewer ran the oracle.
