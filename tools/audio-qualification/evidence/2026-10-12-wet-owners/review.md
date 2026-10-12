# Self-review of continuous owner wet returns

The primary agent implemented and reviewed this patch alone, as requested.
No independent reviewer, live GUI or physical audio device was used.

Reviewed the complete substantive diff, constructor and wire preservation,
core/store/context version admission, captured native targets and scoped edits,
semantic dot/macro behavior, checked owner contexts, per-contribution clocks,
shared nonlinear drive, tile identities, temporal ordering, native workspace
ownership and cleanup after cancellation or resource refusal.

A regression exposed unwanted incoming parent reverb during an explicit Tail
pause: the first sample differed by roughly 0.074. The fix distinguishes Original
and independent sound routes, detects checked Hold/default-gap Tail providers,
and filters the wet history's permissions at the consuming sample. Parent dry
processing remains intact. The original failing executable hash and log are
retained. The fixed test passes for both Tail effects; the default-gap regression
also checks that parent returns resume after the gap.

Continuous owner levels are separate from allocated-Tail sends. They tap after
the complete owner chain and join before parent treatments. The temporal prefix
stops before the owner's own return; descendant returns can feed an ancestor.
One native workspace per effect is shared within a request only after recursive
input preparation. Preparation bounds, temporary histories and cached outputs
are admitted separately. No callback work, dependency or tolerance was added.

Remaining qualification includes wider copy/retained/Preserve/loop cases,
independent event continuous sends, partial attached-sound processing scope,
persistent cache reuse and application playback cost. Existing atomic scope
refusals remain. Full DP-09/DP-16 completion is not claimed.

Clippy caught the enlarged gain proposal crossing the request enum size threshold.
The final payload is boxed at queue ingress; consumers borrow the same proposal,
and ordinary drop owns its lifetime. All 320 project-service tests and strict
workspace Clippy pass after that change. No lint suppression was added.
