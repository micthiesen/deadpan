# Stage-output validation baseline v1

This is the literal ordinary `build_stage` validation capture frozen before M
policy admission. It comes from the shipping copy's optimized native build,
with a byte-identical repeat and independent acceptance of the real reservation,
content, progress, cache and release observations.

`corpus.json` is 4,617,173 bytes. Its SHA-256 is
`3f3d626f064bab86af1102e878d5cdd3f0c3efd55edbe4dc0e10673a9cd8cf9e`.
Keep these bytes fixed.

Persistent keys include the actual source-derived engine pin. Historical replay
must independently hash both literal update transcripts, join each digest to
its actual returned key, compare every non-engine byte, and validate every key
reference and cache-file association. Full capture bodies are not claimed
byte-identical across engine builds. Never force a pin, remove key fields,
normalize the captured body or regenerate this oracle.

The selected boundary discards its policy after content checks and progress,
before the build's saved-key scan and child PCM preparation. Ordinary outer Vec
capacities are observed; admitted borrowed-slice capacities and nested owning
heap capacities remain unexposed. The corpus does not qualify other policy
producers or whole preparation.
