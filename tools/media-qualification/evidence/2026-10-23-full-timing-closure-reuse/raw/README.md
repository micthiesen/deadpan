# Full timing closure reuse qualification drivers

This scratch directory contains adapted qualification drivers for a possible
single-file Core optimization. No new qualification run has been executed here
and this directory contains no current-source identity capture.

The original qualified implementation baseline is ca40995b64fc4acdc0a5e046660a36a1faa7cee5.
The retained baseline source identity map and qualified control binary manifest
were copied from the compact JSON size qualification's release-native records.
The qualification flow is adapted to check the candidate's one-file
audio_reference.rs change.

The copied drivers preserve the existing full qualification sequence, private
execution copies and hashes, source/input/binary guards, physical Mac and
synthetic worker requirements, native/media checks, portable package checks,
and five quiet-load control/candidate timing pairs. The post-driver also checks
all five full_layout_reuse_ tests in the all-Core archive. All outputs remain
pending; no results or production-adoption claim are recorded.
