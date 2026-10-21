# Scratch canonical PCM witness

Prepared 2026-10-20 without repository edits or Cargo execution. This is a proposed regression, not an observed PCM failure. The scout's production path predicts a phase reset, to be confirmed by the root's old-code run.

Source: `ordinary_duration_fixed_retime.rs` in this directory. Append it to `crates/deadpan-audio/tests/stages/ordinary_duration.rs`; no new module registration or harness is needed. It reuses `voice`, `assert_policy_only_duration`, `record`, `assert_pause`, and the deterministic canonical mono 44.1 kHz WAV provider. It is not decoded real-media/export evidence.

Exact filter:
`ordinary_hold_growth_inside_fixed_retime_retains_44100_source_phase_with_unchanged_root_duration`

Literal fixture: root Sequence [silent lead3, Retime output4], ordinary Edit/FollowSpeed Retime mapping [0,4), child Sequence [silent Hold2, natural-rate Source2 starting source3100]. Old root duration7, old Source entry root5/sample8008. Raw Hold2 -> 3 makes child5 but leaves the fixed Retime mapping valid and root duration7. New visible Source entry root6/sample9610, output ends sample11211. The surviving interval is 1601 samples, compared to old [8008,9609).

The test proves the old first256 samples independently using literal source3100, old absolute output origin8008 and 147/160 source-sample step. It asserts the old fixture is valid and has no physical bindings. New first256 must match the literal oracle; all1601 surviving samples and suppression flags must match the old audible prefix exactly. It checks silence up to9610, output length11211, unchanged Source and fixed Retime recipes, root duration7, zero duration delta, exact inverse, JSON reload, and alternate [127,3,251]/[1,239] chunk schedules. There is no binding-layout assertion, inferred offset correction, tolerance, or plan-derived expected phase.

Expected primary failure is new sample9610 versus source3100. If the first256 pass but only the final samples of the surviving prefix differ, audit finite Retime clipping/filter context separately before changing the oracle. The old independent oracle intentionally avoids endpoint DSP.

The minimal fixture has no outside tail so its required total remains exactly7. A separate follow-up fixture can add an audible outside tail to prove unchanged audio outside the fixed Retime, once the primary old-code witness is established. It does not test Preserve, nonunity affine transforms, sounds, marks, shared definitions, or a previously clocked Retime.
