# SDR pixel boundary review

The renderer implementation and its real-Metal example had separate authors.
An independent reviewer examined the owned-plane API, conversion order, bounds,
padding, chroma phase, allocation ownership, cancellation and both callback
orders against the pinned wgpu 30.0.1 sources. The same review covered the
independent reference and exact fixture hashes. No concrete library/example
finding remained before the workspace gate.

The C/Python consumer was independently reviewed for exact byte ownership,
hash admission, plane comparisons, metadata, timestamps, complete encoder and
decoder drain, exclusive outputs, process faults and retained observations.
One concrete finding was the SAR fallback accepting malformed frame [0,0].
The parent replaced it with the existing strict encoder-oracle helper: only a
valid zero numerator with positive denominator can select explicit stream SAR.
The new regression includes malformed and Boolean fields, valid unknown SAR,
non-square stream fallback and independent continuation of the remaining checks.
The independent reviewer confirmed the final correction with no remaining defect.
The reviewer did not execute compilers or tests; the parent owns verification.

The first strict workspace lint found a syntax error in the example's json!
macro: indexing an array literal required parentheses. The command finished
before that one-line correction. Its failed log and source inventory are
retained alongside the subsequent successful lint. No passing tests were
discarded or restarted for that correction.

The first native compile found Darwin's O_NOFOLLOW declaration hidden by the
existing _POSIX_C_SOURCE definition. The parent retained its exact source and
failed compiler output, then enabled _DARWIN_C_SOURCE before all includes,
following the installed SDK's sys/fcntl.h guard. The independent reviewer
confirmed the fix; the no-follow check remains intact. The existing encoder
dispatch receives a separate measured regression control after this header
change. The already passing Rust, Python and Metal runs remain applicable.

The full Rust gate ran against frozen Rust/Cargo sources. Python/C qualification
authoring and documentation were independent of that Rust run. The separate
native harness inventories its complete local dependencies before and after
each measurement. Unchanged preview widgets and interaction paths did not need
another UI harness, imagegen or computer-use session for this backend change.

No mux-policy approval is inferred from these video-only fixtures. The pending
AAC edit-list question and all earlier timing failures remain recorded.
