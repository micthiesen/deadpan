# Independent review: root-sound Trim PCM witnesses

Scope: the frozen `root-sound-trim-audio.patch` test addition only. I reviewed the test implementation, its README, the existing fixture recipe and relevant source-audio mapping/filter/ramp contracts. I did not edit the checkout or run Cargo, native code, or media tools.

## Findings

No remaining correctness finding in the PCM oracles, support bounds, or read sizing.

The staged test helper built a `SourceVideo::Blank` Source with `audio: None`. That shape is rejected by document validation because blank time belongs in a Hold. This was an integration fixture defect, not a production defect. Root reports the applied fixture now uses a declared picture-only Stream; that correction resolves the issue.

## Checks

- The Mono441 terminal sample 9609 witness uses the 44,117-sample fixture. Its exact selected support is `[8822, 8823)`, within the fixture. The shorter 8,197-sample Stereo48 fixture is queried only within its declared support.
- Test reads are at most 256 output samples; independent oracle reads are 251. With the 128-sample kernel halo and the fractional step no greater than one, the configured 512-source-sample limit is sufficient.
- The literal sample/frame arithmetic is consistent: `mix_frames(n,d) = n*5/(d*8008)`, the In-only translated phase is `705159/160`, the equal In/Out direct terminal phase is `1374009/160`, and the retained-gap phase is `46893/32`.
- The exact-Hard pair distinguishes 3203.2 mix samples from 3203.1 even though both round to the same label; both retain the same `[3197,8197)` filter support, so the comparison isolates edge policy.
- The Automatic cut oracle uses sample-centered ramps. The transported end label and new allocation end differ by one sample as documented; its final audible sample uses `3/192` gain. The Hard case keeps the outer Hard policy.
- Prior route chronology, the two inserted silent regions and the retained earlier gap are checked with literal labels/phases. The negative clipped-support oracle differs from full source context.
- The test limits its claim about fractional reconstruction correctly: it checks mapping and support through the canonical kernel, rather than independently qualifying the kernel itself.

This is a static review only. Runtime/build status belongs to the root agent's focused run.
