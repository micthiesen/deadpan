# Freeze the old owner-query oracle, 2026-10-28 (UTC)

Before changing query admission, two pinned old-kernel executables capture
the actual logical query/capture results and numerical WorkControl receipts.
Five test-only files add producers under private test modules; no product
kernel or public inspection API changes. Published expected JSON is copied
byte for byte from the isolated variant.

The five fixture families contain 14 cases: unbound Root and captured Domain;
stable Repeat plays/default gap/overrides; hidden resumed Domain versus live
root; nonunity fractional Shift with PointCeil and a 44.1 kHz source clock;
and actual Source Split after explicitly installing a captured lattice. The
resumed fixture also reaches nonunity Preserve. The hidden Domain differs
from root at the same labels despite coarse identity overlap. Its actual
selected member retains the hidden grid. This is a literal counterexample to
root substitution, not two wrappers over a changed resolver.

SnapshotV1 records ordered spans/owners, selectors, stable plays, current/
retained provenance, affine clocks, captured Input and complete Domain/seed
fields. Private node indices become stable NodeIds; ratios use normalized
integer strings. Plan branding is checked against the actual Plan. No pointer,
Debug representation, raw struct bytes or allocation capacity is an identity.
Old Q/C, probe work and exact/one-short errors remain frozen.

The main and isolated fixture arrays and Plan work receipts are identical.
Independent old numerical-helper captures also have identical case arrays:

| Case | Query Q | Capture C | Probe | Retained owners | Total |
| --- | --- | --- | --- | --- | --- |
| Simple Root | 5 | 8 | 9 (None) | 0 | 22 |
| Actual captured Domain | 5 | 8 | 9 (None) | 0 | 22 |
| Accepted retained input | 16 | 20 | 19 (Some) | 5 | 60 |

The [Plan golden](../../crates/deadpan-plan/tests/fixtures/owner-query-baseline-v1/snapshot.json)
has SHA-256 55dc254e3f7c0791cda320093b0d54dec7058e22e586cc6ef9b95bf2611dd876.
The main logical output has SHA-256
907cfabc0a353cd5e4535061ec6d8ada969f6f4c76e8aaf05f9dc6f705e0f2bd.
Their top-level variant/source-authority fields differ intentionally. The
[Audio golden](../../crates/deadpan-audio/tests/fixtures/owner-query-baseline-v1/work-receipts.json)
preserves the original work-capture generation. No expected output may be
refreshed from new query wrappers.

## Verification and retained correction

Rust 1.97.1, locked dependencies and the qualified development environment.
Initial and corrected lib-only builds each pin Plan and Audio executables.
Guards cover 1,441 main and 1,212 isolated inputs. All six explicit successful
producer runs (logical, work and diagnostic in each variant) execute one
selected test with no ignored test left in that invocation. Producers are
deliberately ignored during ordinary tests because they create external
one-shot captures; their ignored declarations supply no acceptance credit.

Existing main/isolated Audio libraries pass 296/277 with one capture producer
ignored in each. Plan libraries pass 116/103 with two producers ignored in
each. Those three producer kinds were separately executed above. Required
cargo xtask check --no-test passes all-target Core/Plan/Audio/Store formatting
and strict lint. Independent reviews find no oracle defect. Existing media
suites are not repeated for these test-only producers.

The first logical capture in both variants failed before writing a golden:
the new fixture assumed a plain Source Split supplied retained history and
selected the last owner. Source review showed the reference-plan test it
reused makes no such promise. The corrected fixture follows the existing
capture_unbound_audio_bindings lifecycle, explicitly installs a real pre-Split
lattice, then runs the same public Split. It selects through actual old probes.

One bounded diagnostic verifies plain Split's three owners return None,
whereas all three owners with the installed lattice return Some(RootRoundEven).
Discovery selects index 0, and both final case rows independently select the
same index 0. Complete old observations/probes are retained. The candidate
generation2 diagnostic had a literal backslash-n newline caught before
application; corrected gen2a is the only executed second generation.
Initial candidates/manifests and old work goldens stay immutable.

The source audit classifies 20 paths per variant: 18 unchanged product/
reference paths and two test hosts whose only changes are exact producer
module declarations. The initial overly broad byte audit caught those expected
test-host additions; the final audit checks them explicitly. Both positive
Plan binary hashes differ from the known-negative initial fixture binaries,
so source hashes alone are not mistaken for fresh executable provenance.

All 17 runner receipts are terminal with unchanged source/binary guards,
including the two expected initial fixture failures. Corrected logical
captures take 0.31/0.25 seconds; diagnostics 0.05 seconds each. These are
verification durations, not performance acceptance. No media, provider,
native processor, GUI, release or packaging evidence is inferred.

## Next implementation and evidence

The reviewed query contract retains one actual query/capture parent and member,
pairs values with leases through reached Core binding/projection constructors,
and separates canonical/additional work plus local child receipts. One
validation loop uses legacy BTreeSet storage when unmetered and bounded sorted
Vec storage when admitted. Capture/probe stays legacy until its later admission;
old C/probe totals therefore remain the oracle. No cold or batching activation
is authorized by this baseline. Product coverage remains about 85%, ten Complete
requirements and all seven gates open.

The [inventory](../../tools/media-qualification/evidence/2026-10-28-owner-query-baseline-v1/inventory.json)
retains 230 gzip payloads: exact old source/fixture snapshots, all generations,
both binary pin sets per build, 17 terminal receipts, six successful captures,
source/selection audits, independent review and the frozen implementation
contract. Stored/expanded hashes verify: 4,034,015 / 17,803,128 bytes.
Inventory SHA-256:
e7cb86ccaf27fea0abd4c784c2f82e31c2f94b9593e756fb3979add4bbf16e2a.

Raw work: /tmp/deadpan-owner-query-baseline-2026-10-28. Query design:
/tmp/deadpan-original-query-admission-2026-10-28/interface-contract.md.
The Core/Plan implementations are separate scratch work under
/tmp/deadpan-owner-query-budget-2026-10-28 and have no runtime qualification yet.
