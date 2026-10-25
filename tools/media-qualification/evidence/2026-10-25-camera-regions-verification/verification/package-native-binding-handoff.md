# Raw native-report package binding correction

Scratch-only correction. No repository, Cargo, product, network or actual native
project operation ran. Prior4c4fa4 driver, example config, preparation receipts
and earlier handoffs are preserved under package-native-binding-before with hashes
in package-native-binding-before.json.

New package-checks.py SHA256:
1304f07218a07702763e9c45f34e67c67e8fff69f933266a8cf4003ea7d1ca7c
38531 bytes.

## Actual raw-report contract

The driver no longer accepts native_project_pointer or any synthetic report wrapper.
It requires the pinned actual raw schema1 native report with failed=false,
exactly1 regions scenario with failed=false and every recorded check passed.

The actual regions check named
"Private replay project root retained for native QA after worker shutdown"
must occur exactly once, pass, and retain expected flags:
scenario="regions", exclusive_creation=true, cleanup_on_exit=false.
Its actual flags must have the exact same typed values.

The check's actual.documents_root must be an absolute existing real Documents
directory under this task's scratch. Its direct Deadpan child must be a real
directory. Exactly1 direct .deadpan entry must exist there, must be a nonsymlink
directory, and the provided project must resolve to that exact package.
No configurable additional directory or arbitrary JSON pointer is consulted.

The final regions check named
"The atomic region:N Redo and its history replay validate in SQLite"
must occur exactly once, pass, and have expected text
"SQLite head equals the complete displayed document".
Its actual full Core document is retained in native-project-binding.json.
Its revision must match Root's actual baseline identity.
The raw report's fixture_identity_file and fixture_sha256 must agree with
the configured pinned MP4 input.

Read-only SQLite snapshot now explicitly captures state.head_revision,
state.cursor and revisions.document in addition to the existing full SQL dump,
table counts and persistent file hashes. Before proposals, both SQLite's exact
head document and the bundled CLI's project dump must equal the complete actual
document from that final durable native check. After proposals, existing exact
full-database/document/history/byte conservation checks remain in force.

## Verification status

Python syntax compiles. The actual native-regions-first/report.json was read only
as a schema reference and correctly rejected because it is a failed run, before
any project discovery or product command. Its hash is
c16189e8425bd932cdb6404d72f3bbbb2ac78f54453a33d93e52884be1370bde.
No synthetic successful NativeReport wrapper was created and no positive
runtime acceptance is claimed. The upcoming actual successful report is required.

package-native-binding-preparation.json and package-native-binding-review.diff
retain the exact update and static evidence. The example config drops the
obsolete pointer and now explicitly requests an actual successful raw report.
The build/verify stage interface, baseline report/binary/source pins, normal
official narrow bundle build, copied scrubbed checks and official-verifier
incompatibility limit remain unchanged.

This note supersedes earlier handoffs' leaf-project-pointer input requirement.
Root still reviews/authorizes actual execution after its current release build.
