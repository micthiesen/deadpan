# Solo review

The primary agent implemented and reviewed the complete diff alone, including
new production and regression files. No other agent was used.

Reviewed exact signed sample support, current versus retained Original clocks,
plan and scope identity, complete enclosing Preserve input, inner selections,
Repeat overrides and indexed scope filtering, Hold gates, aggregate work/run
bounds, source admission and unchanged ordinary query behavior.

A context begins at the outermost enclosing time/pitch processor, preserving
selections inside it. Transparent crops above that boundary no longer truncate
its input history. The explicit scope follows the requested owner's stable
Repeat prefix; it cannot admit another play just because the full processor
spans it. A billion-play test checks bounded lookup without rendering that DSP
allocation. Membership proof work includes both stable-prefix walks.

Negative history is valid only inside the checked context. Current Hold policy
uses its full structural domain. The existing decoded-PCM reader remains the
source-admission boundary; a warm cache still rejects a revoked provider. Empty
out-of-scope batches validate the source recipe. Ordinary APIs delegate through
the same implementation without a context and retain root delivery bounds.

Decoded-media comparisons cover cold, partial and batch reads, nested Preserve
and inner selection. The NTSC fixture checks fractional source phase and the
last zero sample after the sound's exact endpoint. Its initial expectation
wrongly conflated the sound endpoint with the enclosing rounded allocation;
the retained diagnostic records that failure. Final-source tests and strict
workspace Clippy pass. No unresolved defect was found in this input API.

The API is not yet connected to mixed group EQ. Exterior delivery constraints
must still be applied by that integration, along with saved sound-clock journals
and per-voice final silence/tail permissions. Existing mixed-EQ guards remain.
No stored schema, DSP kernel, existing bus route or native UI changed here.
