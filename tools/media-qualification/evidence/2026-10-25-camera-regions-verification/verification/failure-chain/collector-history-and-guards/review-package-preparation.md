# Package preparation static review

Reviewed `/tmp/deadpan-camera-regions-2026-10-25/package-checks.py` at SHA-256 `1c028843f7849a466443a2904a7581134c902c0fc89052cbe16f94190b89d830`, its example config and handoff, and the relevant current `bundle`, `bundle-audit`, helper probe, doctor, and proposal CLI implementations.

## Findings

None found in this static pass.

The build path invokes the ordinary `cargo xtask bundle` with only the deliberate `--without-ai-runtime` scope flag and the configured qualified FFmpeg prefix, checks a pinned source manifest including `Cargo.lock` both before and after, and fingerprints the resulting siblings and production artifacts. The runtime path copies the exact built bundle with `ditto`, compares full byte/mode inventories, then launches product commands with only a fresh `HOME` and `/usr/bin:/bin` PATH from an empty working directory. Doctor/helper probes verify bundled sibling workers, mapped Frameworks libraries, and bundled helpers against compiled pins; the CLI's helper probe is explicitly offline.

All six comparison cases are explicit, pinned, and check protocol, mode, content/asset/revision, exact PTS/ordinal/stream, engine revisions, and expected counts against both fixed expectations and unbundled reports. Their comparisons exclude only the four top-level timing fields that the current CLI emits. SQLite snapshots read in `mode=ro`/`query_only` transactions and retain a complete SQL dump/schema/row view, with persistent project file hashes and the full project dump checked before and after. The failure `finally` path records project, bundle, input, and source rechecks when those snapshots were reached.

Skipping official `bundle-verify` is recorded explicitly with its AI-runtime incompatibility, and the success state is named `passed_scoped_worker_relocation`; the script does not claim full release or AI-runtime qualification. No code edits or product/build commands were performed during this review.
