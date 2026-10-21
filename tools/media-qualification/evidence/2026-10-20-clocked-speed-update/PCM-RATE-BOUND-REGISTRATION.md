# Scratch rate-bound diagnostic integration

Proposal only until root executes it. No repository change or Cargo run by this author.

Copy `rate_bound_pcm.rs` into `crates/deadpan-audio/tests/stages/rate_bound_pcm.rs` and register it FROM `tests/stages/insert_time.rs`, because it reuses that module's admitted `insert_pause`, `mono_original` and existing stage fixture helpers:

```rust
#[path = "rate_bound_pcm.rs"]
mod rate_bound_pcm;
```

Focused test name:

`insert_time::rate_bound_pcm::validated_clocked_speed_candidate_exposes_retained_transfer_103_before_native_step_limit`

This is an expected-error diagnostic, so success means the TEST-ONLY validated candidate exposes the precise unsupported retained-transfer rate. It is not a proposed product behavior or a passing public SetRetime regression. Keep it separate from root's existing old-code early-refusal witness. The checked JSON mutation changes just the slow owner's exact extent103->1 and covering output recipe103->1. It keeps all child geometry, bindings, lineage, Hold and descendant clocks. It retains the same immutable revision intentionally: it is a scratch hypothetical candidate, not a committed edit or cached renderer reuse.

Literal clocks: Source12 to FollowSpeed100, lead2. Public InsertTime atroot5 length3 creates slow extent103 and input309/25. Left fragment has child9/25, pause adapter9/25, right fragment291/25. Root duration105. Candidate output1 retains input309/25 and yields root3. Left live window is[2,209/103), root RoundEven allocation[3203,3250). Retained left raw allocation is[3203,8008). Old left source first phase3100-441/20000=61999559/20000, step441/4000. Native finite left selection3100..3630. Positive256sample PCM control runs BEFORE candidate construction, is audible and unsuppressed, and checks every scalar exactly. The candidate retained-domain transferstep103 exceeds64, even though composed native step45423/4000=11.35575 remains below64. Actual StageAudio::read must return typed Transfer(Preparation(InvalidRecipe("source step outside 1/64..64"))). Never treat silence or another early failure as proof.

## Separate normal27->15 oracle amendment for later

Do not flatten the old native resampler and new retained-domain transfer into one source sample recipe. Build current finite native Source PCM on each literal retained old grid, apply old physical allocation/suppression, then sinc-transfer it at9/5 onto the current output, then current output policy/envelopes. The proposed Source12/output24 fixture old native step is147/320. Old left reference begins3203; old right begins8008; new right allocation begins8542 atroot16/3. The composed native step1323/1600 is coordinate evidence only. It does not establish equality of cascaded filters. This amendment is not implemented by the rate-bound diagnostic.
