# Reordered video media duration

See the [qualification record](../../../../docs/qualification/mux-duration-2026-10-08.md).

`movie.mp4` is the untouched synthetic PQ Main10 file rejected during the
2026-10-08 workspace gate. `encoded-manifest.json` is its original encoder
contract and report. The fixture preserves the actual VideoToolbox packet
ordering; rerunning the encoder can choose another ordering and miss the defect.

Video `mdhd` declares 47 ticks; `stts` and all 46 pictures span 46 ticks at 30 fps.
The production fix proves the packet-table clock before correcting only the
video media-duration header. Independent output verification stays strict.

`source.json` and `environment.json` identify the final reviewed source and
reference Mac. The compressed check, focused-test and full-gate logs retain
their actual results; `results.json` summarizes the assertions and the one
unexplained UI pipe-closure warning. `bundle-results.json`, release/build
provenance, executable hashes and bundle logs record the relocated release
verification. `manifest.json` hashes every retained file except itself.
