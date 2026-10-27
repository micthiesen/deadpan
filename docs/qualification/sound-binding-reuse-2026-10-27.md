# Historical sound binding reuse, 2026-10-27

Historical binding now reuses structural admission only for the exact shared,
immutable frozen-layout allocation. It still checks the frame rate, processing
capability, selected complete asset contract and source-span containment.
Separately allocated equal layouts retain full validation and structural
comparison. Layouts below four nodes keep the cheaper full path.

After-birth history accepts the caller's already validated binding and checks
the exact live plan, owner, sound and journal ordinal, both historical plan
pointers and the recipe. Complete journal, context, alias, route, placement and
birth checks remain. Equivalent newly retrieved scope handles remain valid.

The fast binding charges fifteen bounded structural operations, including its
new identity checks and the remaining catalog/recipe operations. Small-layout
full execution adds one shape-inspection unit; a storage miss adds three
discriminator units. This removes actual layout validation/index/count work
without increasing limits or adding a cache or dependency. String comparisons
and ordered-map lookups remain bounded; the charge is structural work, not an
instruction or byte-comparison count.

## Verification

Apple M5 Max, macOS 26.5.2, Rust 1.97.1, locked dependencies, qualified FFmpeg,
optimization level 2 with debug and overflow checks. Independent source review
finds no material issue.

| Check | Result |
| --- | --- |
| Current Plan unit/integration suite | 531 pass across 28 executables; zero ignored. |
| Current Audio library | 195 pass; zero ignored. |
| Current Audio definition suite | 171 pass; zero ignored. |
| Isolated five-file patch without unfinished Split work | Builds; 518 Plan, 181 Audio library and eight historical birth/processing regressions pass; zero ignored. |
| Full unchanged real-WAV Split matrix | Four pass, three work-limit failures; 373.33 seconds. |
| Strict check | `cargo xtask check --no-test` passes formatting and strict all-target Core/Plan/Audio/Store lint; tests run separately above. |

Five new Plan tests cover shared versus detached-equal storage, exact budgets,
the cheaper small-layout execution, missing/changed asset contracts, exact plan
handles and unsupported derived clocks. After-birth regression accepts equivalent
fresh scope handles, rejects a binding from another journal ordinal or an
equal-content foreign historical plan, and retains all twelve negative authority
cases. Its complete route remains samples `24024..30431`. Reported work 117 is
accepted exactly, one less fails; rebinding with the new fast path would cost
125. Earlier full-binding work was 20; the fast binding costs 15.

The first broader Audio definition run passed 169 tests and failed two inherited
Split candidate cases. The early empty-occurrence shortcut discarded its event,
bypassing downstream source admission. Keeping an empty prepared event restores
that admission; the same test passes on committed HEAD before this optimization.
The second test still expected no root delivery family although the pending
Split candidate implements certified complementary root delivery. It now checks
equal sibling delivery tokens, distinct retained-input/partial-leaf contexts,
and exclusion of other Repeat plays and Split forests from contribution aliases.
Independent review finds those boundaries intact. Both fixes remain with the
uncommitted Split candidate, outside the independent five-file patch. The final
171-test run passes them and all other cases; original failures are retained.

The incomplete Split comparisons remain all-owner Source/Sequence `cut-three`,
limited `0+511`, and selected/nested selected `prefix-two`, limited `0+4096`.
All-owner still reaches `plan work per read`; selected now reaches
`sound clock scope work` and nested selected reaches `sound gate work`, beyond
its previous binding limit. Processed root-sound refinement still passes its
complete authored/faded/limited comparison. The matrix overlaps one isolated
build and strict lint, so its wall time is not a calibrated performance result.
No requirement, criterion or delivery gate closes.

All 1,390 guarded current inputs and executed binary bytes stay stable during
the final runs. The isolated patch separately guards 1,162 inputs. Raw records
and pinned executables remain in `/tmp/deadpan-binding-reuse-2026-10-27`.
The current matrix executable has SHA-256
`64db5c7ec369c4511be57e312118a40b8eb5d27ffbb9affcfa24b606d3b88174`.
The [evidence inventory](../../tools/media-qualification/evidence/2026-10-27-sound-binding-reuse/inventory.json)
retains the exact independent patch, current candidate reconstruction, commands,
outputs, identities and review. All 271 payloads verify stored and expanded
hashes: 10,552,464 stored bytes and 41,844,089 expanded bytes. Inventory SHA-256:
`f1855078cc7e5820ebf11413dfa8a0d6491caf9a66d32ad550320d803e36e6c0`.

The next candidate is an unused second gate query: ordinary events prepare
post-treatment gates even when event-return processing will never consume them.
Keep the first full gate query, complete source/preflight checks, and all charges
for work still performed. That follow-up is not part of this qualification.
