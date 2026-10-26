# General Split sound research checkpoint

This is an incomplete research checkpoint, not product qualification. See
SUMMARY.json for exact passing and failing runs and important limitations.

Run receipts retain invocation, environment, source pins, streams, exit and
elapsed time. Compressed files use deterministic gzip; SUMMARY.json records
their expanded hashes and sizes. The final candidate patch plus nine new source
files reconstruct the clean candidate from the named Git base. Use a separate
checkout/copy and Cargo target for a source comparison. Never reset this shared
checkout to reproduce an older state.

The trace comparison proves a missing delivered sound contribution in a root
temporal input, rather than acceptable floating-point variation. Do not relax
PCM equality or widen processing-history scope to conceal that failure. Delivery
continuity needs its own checked authority, preserving current source admission,
per-route support and silence permissions. All seven delivery gates stay open.
