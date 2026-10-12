# Independent sound treatments

Real decoded PCM, exact clocks, cold reads, native typed authoring, durable
history, a verified H.264/AAC export and offscreen Metal replay evidence for
core 53/store 81. See the repository qualification for final counts and limits.

Run `python3 check-evidence.py --source` from this directory to verify retained
hashes, final results, exported-file identity and current source. Omit `--source`
after source changes. The base commit and implementation patch identify the
reviewed source; command receipts identify each executed test binary.

All work and self-review used the primary agent alone. Offscreen replay does
not establish physical keyboard, OS input methods, acoustic output or live GUI
behavior. No requirement/gate is closed by this increment.

Failed attempts are retained: the initial Clippy errors; deliberate command
reference regeneration failures; stale frozen-fixture schema fields; a test
command selecting an unavailable feature; and an undersized aggregate-budget
test. The first export fixture's peak expectation omitted the left-channel
attenuation from its 50% right balance; its bounds were halved, retaining the
same relative tolerance. The successful final runs follow their corrections.
The initial placement replay allowed only three wheel steps to reach Remove
sound. Eight bounded attempts cover the taller inspector; the real wheel input
and complete paint-visibility assertion remain. Its report and failure image
are retained alongside the final runs.
