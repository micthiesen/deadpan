# Capture profile preparation script

`prepare_capture_profile.py` prepares a guarded disposable source tree only. It
checks every Rust path in the immutable first-pause source manifest, then reads
only the changed/new per-file overlays under
`candidate-all-core-final-overlays/<relative-path>.txt`. Each overlay must hash
to the corresponding entry in `candidate-all-core-final-source-identities.json`.
The script requires all original Rust paths to remain, including historical
fixtures. It then applies the retained four-file outer timer overlay and adds
coarse scoped-layout and `capture.walk` timers. Timer source snapshots are saved
as `.rs.txt` with exact anchor and identity receipts.

`candidate-all-core-final.tar.zst` is a nextest executable archive, not a source
archive. The script does not inspect or unpack it. Version 1 incorrectly treated
it as source and is retained verbatim as `prepare_capture_profile.v1.py.txt`;
`prepare_capture_profile.v1-attribution.md` records that it was not executed.

Version 2 stops after writing prepared source and receipts under this
`next-profile-plan/` directory. It does not invoke Cargo, launch a binary, or
call Git. The overlay files and prepared trees are scratch copies; no shared
production path is edited. It adds no per-node timer, changes no validation
branch, and leaves the retained raw measurements untouched.

Do not treat timer values as ordinary phase evidence without a paired overhead
control. `capture_scoped` and `admit_sized` timers are nested, so their values
overlap. Fallback and early-refusal paths emit separate labels only when
reached; the coarse complete-scan line is printed after wire counting and the
byte-limit decision. `capture.walk` reports whether the retained-duration fast
path was available. Debug-only full-capture oracles remain in the copied source.

Syntax validation used Python `compile()` only. Version 2 was not executed: no
source copying, overlay application, build, profiling run, or measurement was
performed for this task.
