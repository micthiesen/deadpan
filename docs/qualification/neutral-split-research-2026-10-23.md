# Neutral Split audio failure, 2026-10-23

This is a retained failing result from an uncommitted aligned sound-grouping
candidate. It does not qualify the candidate or close a requirement or gate.

The actual-WAV fixture attaches an independent SelectedPlacement sound to a
four-frame Source at 30000/1001 fps. A five-frame prefix moves it from frames
[4,8) to [9,13). Public `GroupSelection` introduces an ordinary group G around
the complete Source, preserving the earlier Top journal scope. G then receives
an EQ treatment. Public `Split` at the Source's two-frame boundary produces two
complete sound-owner aliases behind disjoint delivery Partitions.

The standalone neutral-Split test has valid controls. It compares the entire
24,024-sample stereo canonical output before and after Split, with alternate
read partitions and cold seam reads. Both the group input and its complete
2,048-sample filter halo are bit-identical, including suppression. The output
changes in 7,581 channel samples between samples 15575 and 19650 inclusive.
The maximum absolute difference is 0.009660936455475166. EQ and event ablations
are active. Full raw halos, output bit patterns, documents and comparison
results are retained in [the evidence directory](../../tools/media-qualification/evidence/2026-10-23-neutral-split/).

The archived `integrated-fifth` run passes six of nine focused audio tests. Its
group-birth copy and ordered-journal authority checks pass. Its later-move
witness cannot establish a separate defect while its neutral-Split control
fails. The late-birth test fails a setup assertion before reaching the intended
audio comparison. Neither failure is presented as a qualified additional audio
defect.

Executed audio binary SHA-256:
`9627184bb29f11f8bb49ee89445b33bfabc3d0744da9d62b87b6920c7f9e29c2`.
The locked, optimized development archive builds successfully in 9.620 seconds;
the nine-test run finishes in 24.002 seconds with nextest exit 100. All 2,031
Rust paths and SHA-256 identities plus six recorded build inputs match before
and after the archived run. The evidence includes these manifests, executable
receipt, build/run logs, 40 frozen candidate source files, and a verified
inventory. Sources are the tested snapshot, not the later repair draft.

The source trace identifies two mechanisms requiring separate repairs. Split
narrows the journal away from its genuine ordinary ancestor G. Shared temporal
processing also assigns separate contribution identities to the two current
sound owners, so one half's consuming permission mask excludes the other
half's FIR input. Restoring G correspondence alone cannot change that mask.
The repair remains in development and has no result in this record.
