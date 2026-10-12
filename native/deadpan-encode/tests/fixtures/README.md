# AAC fidelity regression PCM

`partial-repeat-eq.f32le` contains 20,821 stereo frames of little-endian f32 PCM
at 48 kHz. It is the canonical limited bus of the real-media
`partial_repeat_attached_sounds` CLI fixture, after partial Repeat, copied and
recopied attached sounds, event/owner EQ and durable Undo/Redo. It has no monitor
gain or codec processing. The sound comes from the repository's synthetic stereo
WAV in a project using the test Original; the Original's audio is placed outside
the selected allocation. No external recording was added.

The capture used the qualified `0c445fc3` CLI's public `inspect-audio --limited`
in consecutive 256-frame windows, with the exact final short window. Re-encoding
this input with the pinned native AAC defaults reproduces every presented f32
sample of the retained failing project export bit for bit, establishing that the
capture matches its encoder input. The new regression uses all 33 blocks above
the unchanged -60 dBFS floor, the existing 1.5 dB SNR and 3 dB level bounds,
packet-derived sample positions and both native decoder modes.

SHA-256: `8288435f2075163e8b400b06e647911b8b82fc61d9937b9ad8a7bff64a5b03af`.

[Qualification and capture evidence](../../../../docs/qualification/aac-noise-substitution-2026-10-12.md)
retain the project, source and binary identities, extraction script and complete
codec comparisons. Keep this input fixed when changing encoder behavior.
