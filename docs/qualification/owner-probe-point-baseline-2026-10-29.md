# Actual PointCeil probe provider baseline

The old retained probe now has an additive oracle for the private Structural
AudioSignal inside its actual returned PointCeil domain. The immutable original
capture baseline remains unchanged. Every immutable v1 metadata field, all 16
cases and all 128 ordered observations still match literally.

The [supplement](../../crates/deadpan-plan/tests/fixtures/owner-probe-point-baseline-v1/supplement.json)
is the actual isolated output, 1,554,773 bytes, SHA-256
169d589017e2b89818f2c06099164c056178888ed7e7ee663d611801f66b9d0a.
The main output hash is
8b3c04dfab6f1f0afed2f068520cad3fdc110fbf138a1ff7b27e5f30d016525a.
The common logical hash, excluding only variant/source authority at both levels,
is 7f38ed7988bdc5e04632672befbb13523e1e78ed789b6a7b55ec3507b37e5542.

## Actual observation

The separate supplement producer invokes the existing v1 producer itself once.
Its collector borrows the actual returned retained Input from the same probe
result hook, after raw_domain and set_evaluation. It never calls signal(), clones
the signal, queries again or reconstructs a Domain. The old v1 observations are
unchanged; the new private fields are a separate array.

The exercised Point result is retained_point_voice, work21. Exhaustive private
snapshots record Point placement, reference grid/map/samples and all 11 logical
AudioSignal fields, including support, sampling support, placement transform,
bypass target and Structural provider. Expected-Plan identity is checked before
node-index serialization. Stable IDs replace indices and exact ratios remain
normalized decimal strings.

Only the actual Structural constructor is covered. Empty raw domains and other
provider graphs are unexercised. The collector is thread-local, inactive before
allocation outside its explicit session, and cleared by RAII on unwind. Active
serialization allocates while metadata lives; this is logical/work/error evidence,
not allocation-peak, source/provider, native/RSS or admission qualification.

## Verification and review

All four actual producer invocations pass one test with zero ignored. The final
main and isolated outputs agree after only their runtime metadata is removed;
all three main generations have the same logical output. Default ignored tools
receive no acceptance credit. Final Plan libraries pass 134/121 enabled tests
with four tools ignored. Strict formatting and all-target lint pass.

Review found and corrected three test-oracle weaknesses: output paths now check
their canonical parent remains under /tmp; every immutable v1 field is compared;
and the foreign-Plan regression requires the exact identity panic before index
access. It covers both an identical independently compiled Plan and a shorter
valid empty Sequence. The first attempted shorter fixture used a Source root,
which validation correctly rejected; that failed run is retained. Final bounded
independent review found no remaining issue.

All four staged Rust files equal both compiled variants. Removing only the new
cfg(test) blocks restores both complete runtime files literally; the other two
files are in the existing test-only module. The other 335 original authority
inputs are unchanged. All 15 receipts are terminal with unchanged source/binary
guards, covering 1,459 main and 1,230 isolated inputs. The new supplement and
README were published afterward, separately pinned, and were not producer inputs.

## Evidence and remaining work

The [inventory](../../tools/media-qualification/evidence/2026-10-29-owner-probe-point-baseline-v1/inventory.json)
contains 110 compressed records, 3,177,790 stored bytes and 23,385,925 expanded
bytes. Its SHA-256 is
1039ccb03ae6467357541f28ca0052248b09f9bc5cb19b98e2321c30da6d1308.
Stored and expanded hashes were verified twice. Base revision is
c121fa74a2bdee9d523e98486c011f013c60d82d; scratch work is under
/tmp/deadpan-owner-probe-baseline-2026-10-29.

Complete editorial/probe/raw-domain admission, retained-owner queries, final
inner-charge handoff, caller/node containers and recursive cold-resource closure
remain open. The earlier capture checkpoint's four-pass/three-fail Split result
stands. General sound Split stays disabled; requirement and gate statuses do not
change. The separate future handoff design is excluded from this evidence scope.
