# Scratch package driver handoff

Ready for Root review, not executed against a product.
package-checks.py SHA256:
1c028843f7849a466443a2904a7581134c902c0fc89052cbe16f94190b89d830
30716 bytes.

Only scratch files changed. No repository edits, Cargo, product binaries,
framework requests, helper probes or network operations ran. Verification of
the driver itself was limited to Python compile, --help, and explicit synthetic
scratch SQLite/duplicate-JSON helper checks. No product check is claimed.

## Separate single-use stages

After Root's execution approval:

    python /tmp/deadpan-camera-regions-2026-10-25/package-checks.py build --config BUILD_INPUTS.json --attempt /tmp/deadpan-camera-regions-2026-10-25/package-attempt-1 --execute

Once actual project/report/baselines are supplied:

    python /tmp/deadpan-camera-regions-2026-10-25/package-checks.py verify --config RUNTIME_INPUTS.json --attempt /tmp/deadpan-camera-regions-2026-10-25/package-attempt-1 --execute

The all mode performs both stages with one complete config. A stage cannot
overwrite an earlier attempt. A failed stage stays failed; a new attempt is
required rather than silently retrying it. Without --execute the driver refuses
before creating an attempt or launching any command.

Build requires source_manifest and optionally source_manifest_key,
target_directory and ffmpeg_prefix. Runtime config adds all remaining fields in
package-checks-config.example.json. Every pinned input path is absolute and
requires an actual expected SHA256; example placeholders deliberately cannot run.

Source manifest schema:
    {"files": {"relative/source.rs": {"sha256": "actual..."}, ...}}
A string hash per path is also accepted. source_manifest_key can select another
top-level dictionary from Root's manifest. Only present repository files belong
in that dictionary; removed paths are omitted. Cargo.lock must be included.
Source bytes are copied as .rs.txt/.toml.txt, not only hashed. Current hashes
must match the supplied final manifest before the normal official build and
again throughout runtime verification; build and runtime source sets must match.

native_project_pointer is a list of JSON dictionary keys/array indexes leading
from the actual native report to the retained project's path. The driver requires
that value resolve to the supplied project, instead of guessing a report schema.
Root supplies exact content/asset/revision/stream/default face revision identities.
The6 baseline reports must be actual canonical CLI JSON, with pinned reference
CLI/worker binaries and the same source-manifest SHA256.

## Implemented checks

Official normal cargo xtask bundle --without-ai-runtime, no --no-build, uses
existing target cache. Qualified FFmpeg8 prefix is mandatory. It records old
cache binaries only as old inputs; current bundle binaries are hashed anew.
No AI runtime acquisition or full AI packaging is enabled.

Copies the app with ditto and runs product commands with only fresh HOME and
PATH=/usr/bin:/bin from an empty work directory. Verifies copied content/modes,
packaging marker/minimum, absent AI runtime, strict/deep ad-hoc hardened signature,
official bundle-audit, every Mach-O's otool loads/commands, both CLI/headless App
doctor's actual sibling worker and mapped FFmpeg paths, and bundled helper probe.

Runs exactly the6 configured proposal cases at indexed PTS0/1335/2669 and
ordinals0/40/80, Regions-only/Both, with strict protocol/selection/source identity,
canonical engine/default face revision/objectness2 and expected0/1/2 bounds.
Each report must exactly match its actual unbundled baseline except4 elapsed
timing fields; Regions-only and Both salient collections must also match.
No scripted detector observation or fabricated baseline qualifies.

The actual retained project is opened read-only by the product. WAL-aware SQLite
mode=ro/query_only transaction captures full SQL dump/schema/tables/history/cursor
implicitly through the entire database, before and after; exact project dump and
persistent file hashes must also remain equal. Only project.sqlite-shm and
.writer.lock are identified separately as transient files. Failed cases still
retain final conservation evidence. Source manifest/files, baseline/native-report/
reference binary/fixture inputs and copied bundle bytes are rechecked.

Every attempted command receives exact argv/cwd/relevant env/start/end/exit and
separate stdout/stderr. Normal subprocess argv invokes tool names through PATH;
no shell wrapper/eval, hidden framework launch or polling. Driver/config bytes,
production provenance/SBOM/release/checksum artifacts and all binary/library pins
are retained. Nonzero command exits or mismatched invariants fail the stage.

## Explicit limitation

Official bundle-verify is NOT run. Its current unconditional AI checks are
incompatible with intentionally omitted AI runtime; the skipped command/reason
is a JSON record. A successful runtime stage is labeled only
passed_scoped_worker_relocation, never full bundle/release/AI qualification.

Potential runtime assumptions are assertions, not preclaimed passes. Root still
owns execution authorization, source/project quiescence, actual input selection,
final scope triage and progress artifact updates.
