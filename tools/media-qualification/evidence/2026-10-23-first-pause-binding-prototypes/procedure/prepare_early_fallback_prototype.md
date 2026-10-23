# Early fallback prototype preparation

`prepare_early_fallback_prototype.py` verifies the immutable prepared candidate
against `candidate-all-core-final-source-identities.json`, then creates a
separate scratch source copy and an empty, dedicated future Cargo target path.
It inserts exactly one Rust guard in `FrozenAudioLayout::capture_scoped`, after
precharge, the derived-clock fallback, duration acquisition and the duration
count check. The guard returns `Ok(None)` when the required alias count reaches
`N-1`, leaving the existing complete-capture fallback authoritative.

The inserted comment cites `full-slice-early-fallback-audit.md`. That audit
explains the all-but-one alias proof and the limits: precharge and duration
failure ordering remain intact, and full capture still owns validation and
refusal. The prepared candidate is hash-verified before and after the copy; the
receipt records full before/after Rust hash maps and requires exactly one
changed Rust file. The complete modified source is retained as
`early-fallback-prototype-overlay.rs.txt`.

This is an internal disposable prototype. Core remains 57 and Store remains 86
because the internal branch changes no wire representation or persistence
schema. The script was not executed; it performs no source copying, builds,
benchmarks, or Git calls. Python `compile()` is the only validation planned.
