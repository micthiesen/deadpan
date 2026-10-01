# Editorial-edge follow-up review, 2026-10-01

This is a working record, not final qualification.

1. The first Trim implementation omitted the required new-cut fades. Keep its raw source/filtering oracle, replace its raw-equals-faded assertion with independent sample-centered ramps. Separate authored edge presence from Automatic/Hard policy and raw support.
2. A bound fade must retain the exact authored coordinate of the actual reference-domain leaf. Relabeling it from a binding-ignorant current leaf can falsely merge another voice's Hard policy with a new edge. The implementation now projects the actual bound leaf through its owner's affine transform. A focused opaque seam witness is required.
3. A marked Sequence's start must affect its incident voice only. Intersecting each later voice with the whole group can restart or extend that fade into later children. A one-sample first child followed by a 200-sample Hard-start child exposes the issue.
4. Merely testing the final retained voice extent does not prove incidence. A later Partition can retain hidden Source support covering the earlier group edge. Route ancestor markers along the actual branch containing the authored edge, while preserving markers inside a full owner retained by a later Split. Bound reference phase and current structural phase can choose different inner leaves; the collector must not guess incidence from the wrong leaf.
5. Existing Slip on a transparent Split Partition changes media joins without creating fades. The shared editorial representation must mark both target sides and incident neighbors, while preserving position, duration, raw support and sampling bindings. Add an actual decoded PCM join witness.
6. Store persistence must cover both incident markers and an explicit Hard choice through reopen, two Undo steps and two Redo steps. Root added that test; execution remains pending. CLI now checks the target marker and Redo restoration.

Reviews are independent static evidence until their proposed counterexamples and corrected behavior are exercised by tests. No release gate is complete.
