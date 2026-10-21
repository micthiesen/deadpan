# Independent review, 2026-10-20

`/root/speed_review` reviewed the core crop clipping and silent-Hold shortcut. No remaining concrete finding. Its initial suggestion to clip the candidate allocation to the selected crop was retracted after inspecting actual renderer order: a nonempty raw domain still constructs the transfer before the leaf crop. The production candidate allocation remains the full physical allocation. See candidate-zero-review/design.md for the rejected proposal and source-derived correction.

`/root/speed_audio_review` reproduced the partial-support enclosing PCM failure at the retained `crop-silence` bound (nonempty raw domain, transfer 2003/1000000). This was a real failure beyond the empty Source read. The final review found no remaining issue: direct HoldAudio::Silence without a Repeat gap returns exact zeros and full suppression after normal cancellation, depth, foreign-plan and frame guards; Source and RoomTone retain their rate checks. Tests cover retained/fresh clocks, both offset signs, partitions, provider isolation, public update, inverse and neighboring refusals.

The empty-carrier claims use actual bounded StageAudio PCM reads. They do not claim a million-frame export or listening verification.
