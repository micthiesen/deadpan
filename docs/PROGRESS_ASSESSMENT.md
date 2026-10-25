# Progress reassessment, October 25, 2026

**Assessed requirement coverage is about 83%. Strict completion remains 8/24. All seven delivery gates remain open; picture effect families are 5/13 complete.**

## Why 89% stopped moving

The first 89% observation was October 8, at four Complete requirements. It was carried through 137 observations over 17 days, ending October 25 at eight Complete requirements. There was no fixed scored denominator: a subjective engineering estimate kept being inherited. Some observations recorded research, prototypes or verification checkpoints rather than newly delivered behavior. Counting those observations as completed features would also overstate progress.

Real progress is documented: projects/recovery (DP-01), exact timing (DP-02), YouTube import (DP-14) and local media/sound import (DP-15) closed during that period. Later work added shared audio processing, more semantic selectors, nested timing edits, target authoring and scoped framing, with retained tests, native replays and emitted-media verification. Smaller completions did not close another whole requirement after October 11.

The remaining integration and verification work was underestimated. Structural operations interact with Repeat occurrences, Retime clocks, sound ownership, history, macros and native UI. A backend primitive does not complete those combinations. Narrow fixes, diagnostic work and evidence maintenance took real work while broad product exits remained open. The old percentage concealed that. Its claimed ±5-point uncertainty and average daily rate had no defensible calibration and have been removed.

## Replacement method

The full criterion ledger, citations and remaining work are in [progress.json](progress.json), under assessment. Two independent source/evidence reviews covered DP-01–12 and DP-13–24; the parent reconciled their classifications. This reassessment did not rerun the full application, model corpus or release suite.

- Each of the 24 normative requirements receives equal weight.
- Each has 3–8 major acceptance criteria, 162 total. Score 1 for implemented and locally verified, 0.5 for meaningful confirmed partial implementation or incomplete local verification, and 0 for absent.
- Average within each requirement, then average the 24 scores: 83.1473%, displayed as about 83%. Criteria are not weighted by expected engineering hours.
- Keep criterion identities and scope fixed. Record a versioned reassessment if the denominator needs correction; never split completed work or merge missing work to move the score.
- Gates and effects remain visible separately. They overlap requirements and are not counted again in the numeric denominator.
- Owner-only checks under §29.1 remain on the owner list and do not block local completion. No product scope has been removed.
- A fully scored coarse requirement does not automatically change strict tracker status. DP-12 behavior scores 100%; its Partial status needs a separate closure reconciliation. Runtime optimization, candidate comparisons and performance remain owed under DP-24 and gates.

**83% measures covered acceptance scope. It does not mean 17% of the engineering time remains or imply a near release.** Rounding and criterion judgment limit precision. Historical estimates remain unchanged and labeled legacy; they cannot be compared to this baseline as a delivery rate.

## Requirement scores

| Requirement | Coverage | Complete / partial / absent criteria |
| --- | ---: | ---: |
| DP-01 Project library, history and recovery | 100.0% | 7 / 0 / 0 |
| DP-02 Exact frame, sample and source clocks | 100.0% | 7 / 0 / 0 |
| DP-03 Structural primitives | 91.7% | 5 / 1 / 0 |
| DP-04 Anchors, attachments and occurrences | 78.6% | 4 / 3 / 0 |
| DP-05 Complete keyboard editing flow | 75.0% | 4 / 4 / 0 |
| DP-06 Registers, semantic macros, dot and configurable bindings | 71.4% | 3 / 4 / 0 |
| DP-07 Time/delivery creative operations | 81.2% | 5 / 3 / 0 |
| DP-08 Framing and picture creative operations | 81.2% | 5 / 3 / 0 |
| DP-09 Audio creative surface and intentional dynamics | 62.5% | 2 / 6 / 0 |
| DP-10 Local transcript, refinement and proposals | 64.3% | 3 / 3 / 1 |
| DP-11 Tracking with correction and loss handling | 83.3% | 4 / 2 / 0 |
| DP-12 Local AI Hold behavior | 100.0% | 8 / 0 / 0 |
| DP-13 Model/runtime manager, safe downloads, offline packs | 100.0% | 6 / 0 / 0 |
| DP-14 Single YouTube Original acquisition | 100.0% | 5 / 0 / 0 |
| DP-15 Local Original, audio-only imports, ownership and relinking | 100.0% | 6 / 0 / 0 |
| DP-16 Shared realtime/offline renderer and bounded media paths | 57.1% | 2 / 4 / 1 |
| DP-17 Automatic SDR/HDR encoded output | 78.6% | 4 / 3 / 0 |
| DP-18 Nonblocking workers, cancellation and stale results | 68.8% | 3 / 5 / 0 |
| DP-19 Cache integrity and accepted-media portability | 100.0% | 5 / 0 / 0 |
| DP-20 Focused native UI, keyboard teaching and accessibility | 64.3% | 2 / 5 / 0 |
| DP-21 CLI/JSON engineering API and parity | 75.0% | 3 / 3 / 0 |
| DP-22 Self-contained personal app bundle | 100.0% | 5 / 0 / 0 |
| DP-23 License, SBOM, privacy and security | 100.0% | 5 / 0 / 0 |
| DP-24 Measured performance budgets and diagnostics | 62.5% | 2 / 6 / 0 |

## Work that carries the remaining risk

- Composability matrix: structural timing transforms must preserve sample-phase history, owned sounds, nested Repeat overrides and fixed/pitch Retime clocks. Latest narrow admissions do not complete general scopes.
- DP-09 is broad audio workflow work: sample-resolution range authoring, variable rate, custom silence/tail policy and sound-bearing transforms remain, beyond polishing existing commands.
- DP-10 automatic local alignment is absent; cursor-priority incremental analysis/tracking needs architecture and real integration, not another synthetic success case.
- DP-08 post-insertion parameter edit gap is directly confirmed in source: Keep refuses an existing animated Follow; Camera exposes no independent factor/curve controls.
- Full shared picture/audio effects and mastering, incremental RenderPlan fragments and filtered cache lifecycle remain implementation work.
- Native workflow integration, HDR/output quality and hostile real-worker stress remain local acceptance work.
- Large-project visible edit/pause latency still misses its target; diagnostic median improvements are not a target or p95 pass.
- Model warm reuse, embedding cache and reference-Mac candidate bakeoff/performance remain under DP-24 and Gates A/E/G despite DP-12 behavior scoring fully covered.

Automatic local word-boundary refinement and cursor-priority incremental analysis are missing. Variable-rate DSP and sample-resolution audio editing remain incomplete. Existing Follow creep cannot independently retune its arrival scale/curve. Full effects/mastering and native placement/inspector workflows remain open. The latest measured large-project offscreen first-pause median is 183.389625 ms, so the 100 ms Hold target remains unfulfilled. See [Performance](PERFORMANCE.md) and per-criterion evidence.

## Delivery gates

| Gate | State | Remaining local work |
| --- | --- | --- |
| A | Partial | Full format/color and large workload qualification; Full mastering/filtered cache lifecycle; Risky model candidate comparisons if still required by selection decision |
| B | Partial | Temporal attachment lifecycle; Remaining semantic selectors/operators; Incremental RenderPlan fragments; Complete shared audio rendering |
| C | Partial | Complete pane/selection/inspector and keyboard flow; Mastered audio/playback; Native editorial latency acceptance |
| D | Open | All Section8 operations and recipes completed; All supported scopes/overrides and editable native paths; Every recipe shared preview/export verified; Shared command/help registry |
| E | Open | Complete analysis/tracking/AI contracts and integration for remaining DP10-12 scope; Reference-Mac quality/validation/latency corpus still required beyond existing generation cases |
| F | Partial | Remaining DP17 output/effect/HDR-quality qualification |
| G | Open | Complete all required product operations; Sustained corpus-merging fuzz campaigns; Hostile media/resource exhaustion inside actual native/model workers; Real long-media stress with export; Current release-candidate complete acceptance reruns; Primary-Mac native performance and remaining diagnostic coverage |

Some old remainder lists predate newer evidence. Model memory admission, ordinary real inference/playback measurements, idle CPU and import/distribution workflows exist. Stale statements were not counted as additional missing work. Gates remain open because their broader exit criteria are unmet.

## Reporting from here

The update tool computes coverage from the acceptance ledger and rejects arbitrary percentage overrides. Updates must name criteria whose evidence changed, or say they add confidence without changing coverage. The dashboard shows strict requirement completion, delivery gates and effects alongside the score. It no longer derives velocity from subjective historical estimates.
