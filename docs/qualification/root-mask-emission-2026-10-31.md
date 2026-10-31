# Root suppression destination ownership, 2026-10-31

The actual Root and physical-Domain reader now emits native typed AudioSample
suppression into a separately admitted destination. Input flattened, processing,
policy and optional fade guards remain live. Root stays on its absolute RoundEven
grid; the preceding Point policy handoff remains on SignalSample PointCeil.

The count prepass charges each actual flattened tag independently and counts
each SilentHold tag plus each explicit policy range, including empty tags and
duplicates. It does not move span, envelope or explicit-policy validation.
The original visitor still applies zeroing and retained envelopes in order.
Allocation is lazy at the first suppression event, after that event's original
PCM zeroing. A funded RangeBuilder retains its vector and future Storage Box
through creative fades. At the existing post-fade merge boundary it finishes
into the same owner and performs the funded in-place union. Neither sort nor
merge removes a standalone empty range. Source and destination leases never
join, and the final owner survives request and controller destruction.
Exhausted ranges retain their previous separate Vec.

The old literal collector remains test-only behind a D-only thread-local replay
switch. That switch leaves C Point policy active; historical older switches
explicitly compose it. The 32nd immutable oracle is a byte copy of the actual
published C complete capture, frozen before D code, with SHA-256
afa6038a02c959c7192c09c08a89266c7d52151a5e59a4f69828fbdf67e7414e.
No historical oracle is regenerated from D.

The extra checked standalone physical-Domain case exposes a real capacity
change: its old second public merge copied the one merged range into capacity1;
the native in-place second merge retains the admitted event capacity2. PCM and
the one [0,64) range stay exact, and the full capacity2 remains charged.
The new test records both concrete capacities. No shrink allocation is added
to manufacture the old spare-capacity behavior.

Four adapter tests pass for literal event ordering, standalone empty ranges,
duplicates, malformed late policy and heapless no-event output. Three checked
query tests compile a real Sequence/SilentHold document and use the actual
admitted request. They prove distinct simultaneously live source/destination
pointers, unchanged source ledger, the same destination pointer/capacity through
Box completion and merge, and output survival after request/work/controller
drop. The exact destination ceiling and one-below are set only after actual
source admission; they do not describe the entire public request peak.
Cancellation and a real expired deadline after pending emission dispose the
native payload before releasing the source. No failure injected inside native
creative-fade processing is claimed.

Five public/native tests compare actual Root and physical-Domain PCM and ranges
with D-only historical replay; retain outputs after provider/controller drop;
measure a fresh-controller whole-request resident minimum; prove same-ceiling
held refusal followed by success after release; exercise actual WAV-backed warm
Original prediction and its unchanged cached Arc; distinguish Root-only replay
from C Point policy; and test every measured whole-read work prefix. Early
constructor prefixes preserve actual Plan AudioQueryLimit failures separately
from native Limit errors.

Both builds preserve all 32 immutable payload hashes and reproduce three whole
historical bodies exactly: observer-off 34885733 bytes (SHA-256
af7a292718365c34fa03692e2cbe58a2cfb3c6d444b5ae29d52647e1fdbbff14),
interval 35439545 bytes (f64d3a00a10967cc3822083d21705bd454f2d33ff2198da30e25671092ab5a53),
and C policy 35440148 bytes (afa6038a02c959c7192c09c08a89266c7d52151a5e59a4f69828fbdf67e7414e).
Three final current captures per build agree: 35440251 canonical bytes,
SHA-256 596d738a53f6ef4cbf7870d74b3025504f6289f735ea91f4a38ed45c6661f44a.
All 744 complete outputs remain exact (264 successes/480 typed errors), as do
42 output fields across the ten internal and four transfer cases. The 477
completed content intervals reconcile individually in both lanes; one resident
refusal retains its literal incomplete claim. No policy/collector query fee is
invented from residuals, and inclusive rows are never summed.

All 21 Audio targets pass: 1120 Main/1114 isolated tests, 22 ignored manual
entrypoints; Main's seven unfinished general Split tests remain filtered.
Libraries pass 539/533. Main's required strict check with --no-test, isolated
all-target Audio Clippy with warnings denied, formatting and both hygiene checks
pass. Rustdoc passes with zero executable examples. The final sound-test-only
change relinks one executable; all other 20 exact binaries retain their actual
full-matrix passes. Compiler artifacts, complete input/stream guards and the
one-file source delta are joined explicitly.

The full matrix retained three sound-admission failures at old fixture ceilings
which omitted the new Root owner. Their raw-read tests now measure the actual
reusable public provider-sentinel threshold with fresh controllers, bounded by
the unchanged default. Adjacent refusal/success is checked; at that same selected
ceiling a 256-frame request must refuse and three 128-frame retries must reach
the sentinel. The passing authored-buffer test and all existing cancellation,
dependency and reuse assertions remain unchanged. No native layout fee is
guessed and no production limit is raised.

The first full library run retained five failures. Three tests measured older
constructor/issuer work without the new collector; a fourth compared a frozen
cache optimization witness with active new ownership; a fifth deliberately
removed a policy mask by detaching an ordinary Vec. Historical fee and witness
checks retain explicit D-only replay. The corruption test instead drops the
real owned mask and assigns the allocation-free empty mask. Existing oracle
bytes and old assertions stay intact; new D tests exercise the active path.

New test qualification also retained the first invalid-root fixture failure
(the checked document requires a Sequence root), an incorrect assumption that
every whole-read work refusal is a native Limit (Plan construction can return
its actual AudioQueryLimit), the physical-Domain capacity expectation, and a
test-only diagnostic Debug compile error. These are corrected at their actual
test boundaries; production code and old oracles stay unchanged.

A real observed resource change remains visible in the two 514-frame aggregate
internal cases: their account observer changes from None to a live shell3 after
payloads are released. Whole-case counters record one initialized shell and
three accepted payload reservations totalling 1043 frame equivalents. These
counters do not identify a particular intermediate query. The retained observer is itself an account handle. Scan spending alone
does not initialize an account. PCM and final range values/capacities stay
separate from accounting comparisons.

No whole-preparation claim or new requirement/Gate credit follows. Ordinary
projected-provider and derived bus/equalizer policies, exhausted vectors,
PCM/dependencies and the remaining full product requirements stay owed.
General sound-bearing Split stays disabled.

Main's 2908 and the isolated copy's 2580 other initial named inputs remain
exact. All 19 owned paths match between builds. The isolated source remains
the shipping authority, including all 2599 named inputs. Independent production,
new-test and capture reviews retain their findings and resolved corrections.

Next isolate the non-separate SharedMix permission-grid output producer. Keep
projection and source-grid disposal inside its existing live reservation
closure, carry the resulting owner through WideBlock and remove the Plain
rewrapping at the recipient. Separate Components output and projected
PreserveTape output remain distinct later producers. The prospective design
and review are design-only evidence, not implemented or accepted behavior.

Evidence: [compressed inventory](../../tools/media-qualification/evidence/2026-10-31-root-mask-emission/inventory.json).
Inventory SHA-256: 51ec9fd0ede2f0bd4baa08fa4f966cb280d411ba3911660dde86ecb5f31b4469.
Its 620 records retain 5125270158 expanded bytes in
486836458 stored bytes.
