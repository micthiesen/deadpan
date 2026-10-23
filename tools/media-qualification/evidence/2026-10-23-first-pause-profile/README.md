# First-pause diagnostic profile

The [qualification note](../../../../docs/qualification/first-pause-profile-2026-10-23.md)
describes the method and limitations. These are short diagnostic runs, with no
performance-target PASS or optimization claim.

`records/source.json` pins the immutable Git source archive. Source hash maps,
four initial timer overlays and the later full Store overlay reconstruct the
instrumented variants. All Rust overlays use `.rs.txt`. Binary manifests pin
the exact copied executables; the executables and source archive themselves
remain outside Git.

`controlled/` and `deep-controlled/` retain all command output, edit-start load,
raw timings and phase multiplicity checks. `records/` also retains the initial
batch above load 3, build receipts and scripts. Setup copy/anchor failures are
preserved separately. Large text/JSON records use deterministic gzip.

Packages contain generated Background/Silence Holds and no source media. Their
file hashes are retained, together with the exact fixture-generation commands;
the large SQLite files themselves are not included. `inventory.json` covers
every other retained payload by size and SHA-256.
