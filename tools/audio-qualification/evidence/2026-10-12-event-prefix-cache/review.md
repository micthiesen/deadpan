# Self-review

Production changes are confined to event_returns.rs. Typed cache stages separate
EQ, reverb, delay and ordered prefixes. A prefix miss evaluates only the earlier
recipe, so recursion terminates at raw input. The prefix key also retains voice,
graph parent, input allocation, audible interval and edge policy.

All prefix samples stay on the original event clock. The existing delivery route
and current gates consume the resulting samples afterward. No decoded sample,
authored order, input-edge application or send level changes. Tests compare
arbitrary cold windows and reordered treatments to independent full-input PCM.

Prefix tiles are memory reuse, not another temporal processor: they retain the
input's depth without adding one or consuming a temporal-node slot. Their PCM
and temporary storage use the existing residency reservation, prepared-frame
accounting and cancellation/deadline checks. All limits remain unchanged.
The graph ends at the request; it cannot reuse a recipe across revisions.
Input admission still precedes reads and cached blocks carry dependencies.

The new callback test fails on the prior implementation's plan-work bound and
passes with shared prefixes. Its first attempted reference exceeded the reader's
per-call bound; that test setup was corrected to bounded reads. A whole-input
cache alone was insufficient; retained tracing located repeated pre-EQ gain
work. No diagnostic tracing remains in production source.

No independent reviewer was used, as requested. Physical playback, nested-loop
coverage and persistent cache/performance qualification remain outside this
increment's evidence.
