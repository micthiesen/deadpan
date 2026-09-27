# Sound sample clocks and source residency, 2026-09-27

This increment adds exact physical-grid evaluation of sound routes, identifiable
current silent-Hold rules and LRU source eviction for playback. It does not
persist or place sound events, add voice effects or complete the final mix.
Core 28 and database 34 are unchanged. Every DP requirement and delivery gate
retains its previous open or partial status.

## Implementation and review

`SoundRippleMap::locate` returns the complete selected immediate-input interval,
including exact seam bias and compact Repeat input strides. `AudioSoundRoute`
retains each chronological grid and resumes old sample phase at a new physical
anchor. Root RoundEven and intrinsic PointCeil remain distinct. Windows keep the
complete recipe, including context needed for later processing. They also keep
the old selected audible sample mask independently of that context.

Independent timing review found that a displaced grid could allocate an extra
output sample and expose audio after the old selection's cut. Both Window and
Keep now pass their old physical half-open mask through recursive evaluation;
any extra sample is a gap. A regression uses a prior grid origin of 1/2, spacing
1 and selection `[0,3/2)`: the old audible allocation is `[0,1)`, although its
new placement allocates `[0,2)`. The sample at the excluded old endpoint stays
silent. A separate matrix compares 375 signed/fractional grid combinations
against independently materialized sample-copy histories. Timing and general
reviewers rechecked the fix and found no outstanding issue.

Hold queries retain node/Repeat-gap issuers, stable preceding plays and definition
namespaces. They do not convert source exhaustion into Hold policy or reconstruct
historical rules from timing records. The existing scalar Original policy is
unchanged. Authored sound allowances remain required.

Playback now keeps at most 16 resident decoded sources, 1 GiB of physical PCM
and 1,000,000 audio index frames, with LRU eviction. It verifies a cold original
before evicting, reserves capacity before decoding and publishes counts only
after successful preparation. A later decode failure can leave victims absent;
retaining them during decode would break the hard reservation. The reviewer
withdrew that proposed finding after checking the explicit contract and counters.

The cache test registers 17 distinct qualified AAC originals, made by appending
different numbers of valid empty MP4 `free` atoms to the existing fixture. It
checks every decoded original against its own receipt, touches the oldest entry,
evicts the next LRU entry and reopens an evicted source. Cancelled and revoked
cold reads preserve the resident set. Numeric tests cover exact resource limits.

Early focused runs exposed invalid fixtures: a no-audio Source with no picture
content, and repeated aliases that registration correctly deduplicated. Those
fixtures were corrected without weakening admission. Strict lint also found two
unnecessary clones of a Copy identity in tests; they were removed. Original
failure logs are retained with the corrected runs.

## Verification

Focused checks passed: 19 core route tests, 22 plan sample-route/Hold-policy tests,
and 3 real-media playback cache tests. The 22 plan tests include compact billion
period queries, exact budgets, signed grids, fractional windows, chunked/shuffled
reads, stable gap identities, definitions and nested retimes.

The complete gate ran against 577 unchanged source/configuration hashes:

| Check | Result |
| --- | --- |
| Workspace formatting and strict Clippy | Passed |
| Workspace tests, including documentation tests | 1,776 passed, 4 failed, 0 ignored |
| Workspace build and CLI doctor | Passed |
| Strict app Clippy with `ui-harness` | Passed |
| App tests with `ui-harness`, all targets | 236 passed, 0 failed, 0 ignored |

The workspace failures match the prior audition increment. The socket fixture
fails at `crates/deadpan-jobs/tests/artifact.rs:200` with `PermissionDenied`
before it can test the socket. Three playback cases fail at the ten-second wait
in `crates/deadpan-playback/src/tests.rs:256`:

- `canonical_source_pcm_uses_fixed_monitor_gain_and_delivery_clock`
- `original_preserves_leading_audio_trailing_audio_and_exact_rounded_picture_clock`
- `original_pcm_ignores_edits_and_target_cache_never_reuses_sequence_pcm`

Each playback case then passed in isolation with the unchanged workspace test
binary. Its SHA-256 remained
`aeee9c337972aff60b89f4b67bfefe1d0b839a76ac15465d21533e1e269eb77e`.
These retries do not erase the parallel failures or establish their cause. The
full gate remains failed; thresholds and assertions were not weakened.

The host was macOS 26.5.2 (25F84), arm64, with Rust/Cargo 1.97.1 and the pinned
developer FFmpeg prefix `/tmp/deadpan-ui-ffmpeg/prefix`. The working-tree baseline
was `c03a5edde5f28d27074745eb15711cb28b1f2e50`; this increment is uncommitted.
[Machine-readable evidence](../../tools/media-qualification/evidence/2026-09-27-sound-clocks/verification.json)
retains complete commands, timings, compressed logs, source hashes, review
decisions and the exact increment diff. The local checkpoint preserves the
tracked patch and all untracked files, including the contributed harness.

## Scope and remaining evidence

No application screen, key binding, schema or authored command changed. GPU
replay, release responsiveness and native physical input/listening checks were
not repeated for this backend increment. The prior
[audition qualification](sound-audition-2026-09-27.md) retains the actual Metal
`No adapter found` failures before scenario execution. The 11 ImageGen boards
and prompts remain visual targets; no new GUI matching or acoustic result is
claimed here.

The next integration must persist complete recipes and physical projections,
resolve scoped allowances, process each voice continuously, and sum the complete
bus before the shared limiter. See [sound events](../SOUND_EVENTS.md). The
contributed harness remains in the tree. Git metadata is read-only in this
session, so no commit or push was possible. The project goal remains active.
