# Reverse Roll oracle correction review

Read-only, 2026-10-01. No execution or production changes.

Confirmed. The right Source maps 48 kHz at unity rate with a fractional phase, so `ExactSourceResampler` uses its 128-sample sinc radius rather than the integer identity shortcut.

The old selection begins at source sample 5983. Probe 6506 starts at `30413/5 = 6082.6`, only 99.6 samples after that boundary. Its tap interval reaches below 5983. Reverse Roll expands support to 4382 while preserving that same phase, so these newly available taps can change the waveform. The corrected expanded-support oracle and `assert_ne` against the former support distinguish filtering from a clock shift.

Probe 6606 starts at `30913/5 = 6182.6`, 199.6 samples after the old support start. Its first possible tap is 6055, already inside the old support; its final tap also remains below the shared end 8197. Both support windows therefore produce the same PCM across the entire 256-frame probe. Comparing before and after against that literal oracle establishes retained phase outside the changed halo.

The correction preserves exact comparisons, independently specified support/phase, both after-render chunkings and the prefix phase negative control. Production changes are unwarranted on this evidence. The before probes currently run once at chunk 193; the two-chunk loop covers after. That is adequate for this bounded correction and should not be described as before being rerun in both chunkings.
