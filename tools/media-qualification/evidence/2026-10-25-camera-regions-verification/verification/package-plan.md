# Scoped picture-proposals worker relocation plan

Prepared read-only from current docs/PACKAGING.md and xtask sources. No bundle
build, Cargo, product execution, downloader operation, network/model acquisition,
repository edit or bundle mutation ran during preparation.

## Scope and known official-verifier incompatibility

Build an ordinary relocatable ad-hoc production bundle with the explicit
--without-ai-runtime option. This is a scoped worker relocation qualification on
this Mac, not a full bundle/release/AI-runtime pass. Existing full AI packaging
qualification remains separate.

Do NOT run this known-invalid combination:
cargo xtask bundle-verify APP --fixture FINAL_SALIENT_MKV --keep

Current crates/xtask/src/bundle/verify.rs::exercise unconditionally invokes
ai_runtime_checks, which requires AI identity, Python imports and Metal even when
the bundle was intentionally built without its AI runtime. Keep source excerpt/
hash evidence and label the official verifier skipped as incompatible. Do not
patch the verifier, suppress its failures or reinterpret a failure as a pass.

## Target and input choices

Prefer CARGO_TARGET_DIR=/Users/michael/Code/deadpan/target for the official build.
Its bundle/release contains all5 shipped executables from a prior normal build;
package-existing-cache.json retains their old hashes/timestamps. They are cache
inputs only, NOT current-source executable evidence. The official command runs a
normal locked Cargo release build into that target's bundle child and obtains
executables from Cargo artifact messages. Do not pass --no-build. This avoids
rebuilding an entirely empty bundle child beneath the current /tmp Cargo target.
Root can instead select the existing current target's bundle child if avoiding
the repository's ignored target cache is preferred.

Use the qualified8 prefix only:
DEADPAN_FFMPEG_PREFIX=/Users/michael/Library/Developer/Deadpan/ffmpeg-8.0.3-opus-1.6.1-dav1d-1.5.4-qualified-3/prefix

Final absolute fixture:
  /Users/michael/Code/deadpan/native/deadpan-track/tests/fixtures/salient-regions.mkv
  SHA256 cc181467a19d92c3b08edf72ff34d4eaac6d4887e8e05311305ed51db7576f66
  635038 bytes; video-only120 FFV1 pictures; tbn1/1000; terminal4004ms.

Use Root's quiescent retained native project, containing that exact Original
and all authored acceptance/history from the real replay. Do not create a
replacement project to simplify the proposal checks. The optional asset argument
must name its actual Original, or be omitted when its one-Original profile
resolves it. Store/cursor/history assertions use this same project.

## Execution after Root releases the lane and supplies runtime inputs

1. Create a new scratch attempt directory, never overwrite a prior attempt.
   Capture exact argv/env, UTC/monotonic duration, stdout/stderr, OS/SDK/compiler
   identities, source-list hashes, input fixture/project/report hashes and old
   cache binary hashes. Snapshot the exact relevant source bytes as .rs.txt.
   Hash source inputs again when all execution ends; any change prevents claiming
   one frozen-source qualification.

2. Official normal production bundle command, from repository:
   cargo xtask bundle --output ATTEMPT/build --without-ai-runtime
   Default helpers root is the installed verified managed helper root. No
   --identity, notarization, AI models, --no-build or downloader install/update.
   Retain its ad-hoc signature, provenance, SBOM, release manifest, SHA256SUMS,
   audit and helper checks. Failure remains a failed attempt; preserve output.

3. Copy ATTEMPT/build/Deadpan.app with bare ditto to
   ATTEMPT/relocated/Deadpan.app. Create new empty ATTEMPT/home and ATTEMPT/work.
   All follow-on product commands use cwd=work and exactly
   HOME=ATTEMPT/home, PATH=/usr/bin:/bin, with no inherited env. The copied bundle
   is the runtime authority; no compiled prefix, Cargo, Homebrew, model path or
   helper override is present in product execution.

4. Hash every file in the copied bundle, record every Contents/MacOS binary and
   Contents/Frameworks library hash/size. Compare copied bytes with built bundle.
   Verify Info.plist DeadpanPackaging=xtask-bundle-1 and minimum15.0. Assert
   Resources/ai-runtime is absent, as explicitly requested.
   Run codesign --verify --deep --strict on copied bundle and record codesign -d
   --verbose=4 output (ad-hoc+hardened runtime). Run official bundle-audit on the
   copied bundle, retaining all output. Run otool -L/-l for copied Mach-O
   executables/libraries and preserve load commands. Require no non-system
   dependency/search path that resolves outside the copied bundle.

5. Run copied deadpan-cli doctor and copied deadpan-app --headless doctor under
   that scrubbed environment. Require packaged=true; all5 shipped tools exist
   inside Contents/MacOS and each reported sibling worker resolves there;
   every actually mapped FFmpeg library is inside copied
   Contents/Frameworks. Require downloader source=bundled and every helper path
   within copied Resources/helpers. Run copied downloader status --probe, require
   helpers verified and probe matches_pins. These are local identity probes;
   no network media acquisition or helper update.

6. Before any proposal operation, read-only SQLite transaction snapshot of the
   retained project's project.sqlite (mode=ro, include WAL normally, not immutable)
   records logical full-database dump hash, schema, all table counts, head revision,
   history rows and history cursor. Retain exact project dump JSON through the
   copied CLI. Also inventory/hash the project's ordinary persistent files,
   distinguishing SQLite transient shm/lock files from authored data. No writer
   or still-running replay may mutate the package during this check.

7. Through copied deadpan-cli, run6 cases and retain each stdout/stderr separately:
   detect-regions PROJECT --at 0
   detect-regions PROJECT --at 0 --include-faces
   detect-regions PROJECT --at 1335
   detect-regions PROJECT --at 1335 --include-faces
   detect-regions PROJECT --at 2669
   detect-regions PROJECT --at 2669 --include-faces
   Supply --asset ACTUAL_ASSET only if required by Root's retained project.

   Require exact protocol2, selection salient_regions/default or both/include,
   Original content identity, asset, saved revision, stream tbn1/1000/rotation0,
   exact indexed pts0/1335/2669 and picture0/40/80. All salient runtimes must have
   canonical Apple Vision objectness engine and revision2. Both additionally
   requires canonical face runtime (actual recorded default revision3 here) and
   faces[]. Default must have faces=null. Expected salient counts0/1/2.
   Compare all region geometry/numbering and per-kind runtime/provenance against
   Root's actual unbundled final-source baseline reports, and compare each
   default result with the same-picture Both salient collection exactly.
   Omit only elapsed/decode/vision/worker timings from semantic equality.
   Do not fabricate reference reports from expected boxes.

8. Repeat the same full logical SQLite/project JSON/file snapshots. Require exact
   authored document, saved targets, revisions, history rows/cursor and persistent
   project bytes unchanged. Require copied bundle bytes unchanged. Retain all
   actual binary, library, script, source, fixture, baseline and report hashes.
   Any discrepancy fails the check and stays in its own attempt directory.

## Runtime inputs still needed from Root

- Explicit Cargo lane release/execution go-ahead.
- Quiescent real native project absolute path; actual Original asset if needed.
- Final unbundled baseline CLI path+binary hash and its sibling worker hash, or
  actual6 canonical detect-regions JSON reports bound to that project/current
  source revision. Include the native replay report that identifies the project.
- Final source manifest/list to freeze all files relevant to the production build.
  Root's selected target directory if different from the cache recommendation.

No clean/second-Mac, full AI runtime, acoustic/human quality, notarization,
distribution, comprehensive UI or general-object accuracy claim follows.
