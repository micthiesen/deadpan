# Section 8 coverage inventory

Date: 2026-10-04. This is a living inventory for Gate D in
[Requirements](REQUIREMENTS.md): every Section 8 operation and starter recipe,
per-play overrides, tails, stretch/pitch, cutaways, framing, saved gags,
registers, semantic macros and the shared command/help registry. Update a row
in the same change that alters its status.

**Summary: 65 implemented, 20 partial, 10 missing (95 rows).** Rows overlap on
purpose: a Section 8 operation, its comma key and its §6.4 command example are
counted separately because each is a distinct obligation.

## Definitions

- **Implemented**: reachable from the keyboard or command language in the
  native app, persisted in the document, undoable, and rendered by the shared
  `RenderPlan` picture path and authored audio bus, so preview and export use
  the same evaluation. "Composed" means the spec's construction is reachable by
  combining existing native primitives without a dedicated command.
- **Partial**: some of those properties hold (for example core and headless
  JSON only, or only some parameters).
- **Missing**: no authored construction exists.

No row is complete for Gate D. Finished-file verification
([FINISHED_FILE_VERIFICATION.md](FINISHED_FILE_VERIFICATION.md)) checks
structure and decode validity of the emitted movie. The
[preview/export harness](PREVIEW_EXPORT_VERIFICATION.md) (`verify-export`)
compares exported content with the committed preview path: per-frame PSNR and
structure against the shared picture path at the SDR encoder pixel boundary,
and per-window level, SNR and exact zero audio offset against the limited
audition bus. The **Export verified** column names the recipe fixture that was
exported through public Render and passed that comparison
([qualification](qualification/preview-export-2026-10-04.md)). Fixtures are
built through the headless command API; "Construct" on a key or command row
means the authored construct that key produces was verified, not the key path
itself. "No" means no export comparison exists for the row; the shared plan
still evaluates it by construction. The `render` replay
([render.rs](../crates/deadpan-app/src/preview/harness/render.rs),
`actual_export`) separately publishes a beat gain trim of -6.25 dB from the
native app.

## 8.1 Time and delivery

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| Dead air | Implemented | `,h` (count scales 0.5 s), `:hold 1.5s` inserts a freeze Hold with `HoldAudio::Silence`; `:hold-duration Nf` adjusts it | [INSERT_TIME.md](INSERT_TIME.md), [NATIVE_WORKSPACE.md](NATIVE_WORKSPACE.md); replays `nested-pause`, `keymap`, `gags` | | [`freeze-hold`](qualification/preview-export-2026-10-04.md), [`black-pause`](qualification/preview-export-2026-10-04.md) |
| Frozen stare | Implemented | `,h` freeze of the measured picture; Camera (`,f`) on the Hold; `:room-tone` / `:hold-silence` for audio; `:gag nothing-happens` | [CAPTURED_FRAMING.md](CAPTURED_FRAMING.md), [ROOM_TONE_AUDIO.md](ROOM_TONE_AUDIO.md); replays `nested-pause`, `room-tone`, `camera`, `recipes` | Export of Camera on the Hold | [`freeze-hold`](qualification/preview-export-2026-10-04.md), [`nothing-happens`](qualification/preview-export-2026-10-04.md) (freeze, silence and room tone; Camera on the Hold not exported) |
| Living stare | Partial | `,a` / `:generate`, `:preview-ai`, `:accept-ai` on a Hold; acceptance leaves Hold audio unchanged | [AI_HOLDS.md](AI_HOLDS.md), [GENERATED_HOLDS.md](GENERATED_HOLDS.md); replay `ai-pause` | Distributed model runtime (generation needs the development MLX harness); candidate sound audition; scoped Repeat/Retime entry | No |
| Micro-loop | Partial | Visual range + `r` repeats a short fragment; or `:cutaway fit=loop` loops a moment over a Hold | [REPEAT_SELECTION.md](REPEAT_SELECTION.md), [CUTAWAYS.md](CUTAWAYS.md); replays `repeat-operator`, `cutaway` | Explicit seam treatment in the app (`set_audio_edge` is headless only); a Repeat nested inside a Hold | [`repeat-with-gap`](qualification/preview-export-2026-10-04.md) (linked Repeat) |
| Ping-pong hold | Implemented | `:ping-pong 12f` inserts at the Edit cursor a pause playing the frames before it backwards without repeating the turning picture (`HoldVideo::Reverse`, `HoldAudio::Reverse`); recordable (`InsertReverse { bounce }`) | [REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md); `reverse_and_ping_pong_insert_the_host_provider_with_their_exact_lengths`, `a_reversed_hold_plays_its_span_backwards_then_holds_the_first_picture`; replay `hold-effects` | Reversing composite structure or other speeds; several bounces in one command | [`ping-pong`](qualification/preview-export-2026-10-04.md) |
| Word / syllable stutter | Partial | `r` + `iw` / motion, `3riw`, Visual `r` (linked); `:repeat N gap=120ms [gap-step=-40ms]` sets held gaps between plays | [REPEAT_SELECTION.md](REPEAT_SELECTION.md), [REPEAT_GAP_BRANCHES.md](REPEAT_GAP_BRANCHES.md); replays `repeat-operator`, `transcript`, `recipes` | Audio-only repeat (§6.5) | [`repeat-with-gap`](qualification/preview-export-2026-10-04.md) (linked Repeat with gap) |
| Escalation | Partial | `,e`; `:repeat 3 gap=120ms gap-step=-40ms gain-step=3dB zoom-step=0.08 [progression=multiply]` changes count, gaps and steps in one Undo, or wraps a plain beat first | [REPEAT_ESCALATION.md](REPEAT_ESCALATION.md), [GAGS.md](GAGS.md); `escalation_scales_each_later_play_and_its_gap_inside_the_repeat_framing` ([picture_plan.rs](../crates/deadpan-plan/tests/picture_plan.rs)), `repeat_escalation_adds_its_step_to_each_later_play_and_its_gap` ([gain.rs](../crates/deadpan-audio/tests/stages/gain.rs)), `one_repeat_change_wraps_or_sets_plays_gaps_and_escalation_together`; replays `editing`, `recipes` | Speed progression; target-centered zoom | [`escalating-repeat`](qualification/preview-export-2026-10-04.md), [`repeat-gaps-steps`](qualification/preview-export-2026-10-04.md) (gaps, gain and zoom steps in one `SetRepeat`) |
| False start | Implemented (composed) | Original `v`/`y` short prefix, `p`, `,h`, then `v`/`y`/`p` a longer span | [SOURCE_MOMENTS.md](SOURCE_MOMENTS.md), [INSERT_TIME.md](INSERT_TIME.md); replays `original-moment`, `nested-pause` | Dedicated recipe; export fixture | No |
| Interrupted answer | Implemented (composed) | `s` split, then `,h` or `:cutaway`, and `d`/`dd` to omit the ending | [STRUCTURAL_SPLIT.md](STRUCTURAL_SPLIT.md), [CUTAWAYS.md](CUTAWAYS.md); replays `delete-range`, `cutaway` | Dedicated recipe; export fixture | No |
| Callback | Implemented | `"a` + `y`/`d`, later `"a` + `p`/`P`; `,g` names the group | [NAMED_REGISTERS.md](NAMED_REGISTERS.md), [EDITED_SLICES.md](EDITED_SLICES.md); replays `named-registers`, `place-slice` | Named-range (mark range) paste selector | No |
| Reverse hiccup | Implemented | `:reverse 8f` inserts a pause playing the stretch before the cursor backwards, picture and sound, then forward content continues | [REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md); `reversed_hold_plays_its_source_backwards_then_silence` ([hold_effects.rs](../crates/deadpan-audio/tests/stages/hold_effects.rs)), `a_reversed_44100_span_with_fractional_extent_ends_exactly_one_mix_sample_before_its_end`, `a_reversed_span_is_bounded_by_the_one_block_dsp_limit`, `heard_sound_must_be_at_its_natural_rate`; replay `hold-effects` | Reversing a framed group or Repeat, or sound under a stretched mapping or speed stage (refused); a key binding | [`reverse-hiccup`](qualification/preview-export-2026-10-04.md) (reversed click 3,250 samples into the pause) |
| Slow delivery | Implemented | `:retime 0.75 pitch=preserve` / `pitch=tape`, `:wrap-retime`, inspector Change speed | [RETIME_EDITING.md](RETIME_EDITING.md); replay `retime` | Preserve stage limited to about 21.8 s input; variable rate | [`retime-half`](qualification/preview-export-2026-10-04.md) (50%, Preserve) |

## 8.2 Attention and picture

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| Smash zoom | Partial | `,z` (1.35x step that follows the selected target, or at the current center when none is saved); `:zoom S target=current\|center\|ID curve=step`, optionally over the Edit range inside the beat | [FRAMING.md](FRAMING.md#zoom-and-creep-commands); [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)) | Face/region proposals for `target=face:N` | [`framing`](qualification/preview-export-2026-10-04.md) (static 1.35x) |
| Slow creep | Implemented | `,c` (to 1.35x over the beat or the Edit range inside it); `:creep from= to= target= curve=smoothstep\|linear`; `:zoom S curve=linear`; gag `long-answer creep=` | [FRAMING.md](FRAMING.md#zoom-and-creep-commands), [GAGS.md](GAGS.md); [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)); `the_long_answer_inserts_a_pause_and_creeps_on_it_in_one_transaction` | Creep that keeps following a moving target (live follow with a scale envelope); creep over an entire Repeat from outside it | [`framing`](qualification/preview-export-2026-10-04.md) (smoothstep creep) |
| Escalating crop | Implemented | `zoom-step=` on a Repeat (centered, additive or multiplicative) | [REPEAT_ESCALATION.md](REPEAT_ESCALATION.md); `escalation_scales_each_later_play_and_its_gap_inside_the_repeat_framing`; replay `editing` | Target-centered or center-moving increments | [`escalating-repeat`](qualification/preview-export-2026-10-04.md) |
| Reaction cutaway | Implemented | `,r` picks a register holding an Original moment (opens `:cutaway register=` and lists them); `:cutaway register=r fit=hold\|loop\|gap audio=keep`, `:cutaway clear` | [CUTAWAYS.md](CUTAWAYS.md); `a_cutaway_replaces_the_host_picture_in_its_range_and_holds_its_last_picture`, `a_cutaway_survives_a_split_through_its_range_with_every_picture_unchanged` ([picture_plan.rs](../crates/deadpan-plan/tests/picture_plan.rs)); replays `cutaway`, `recipes` | Ranges across beats; cutaways over groups or one Repeat play | [`cutaway`](qualification/preview-export-2026-10-04.md) (Hold fit) |
| Reaction ping-pong | Implemented (composed) | Several disjoint `:cutaway` ranges on one host (sound kept), or alternating pasted Original moments (sound switched) | [CUTAWAYS.md](CUTAWAYS.md), [SOURCE_MOMENTS.md](SOURCE_MOMENTS.md) | Single alternation command; export fixture | No |
| Off-center stare | Implemented | Camera `h/j/k/l`, `+`/`-`; draw a target with `n` and follow it with `t`; `:framing-save a` keeps the selected beat's framing as a reusable preset that `@a` / `:macro a` applies to another beat | [TARGETS.md](TARGETS.md), [FRAMING.md](FRAMING.md); replays `targets`, `recipes` | Target rename/delete in app; targets end at the next shot boundary; presets live in macro registers rather than a named preset list | [`off-center`](qualification/preview-export-2026-10-04.md) (static off-center pose), [`framing`](qualification/preview-export-2026-10-04.md) (follow of a moving target) |
| Freeze a detail | Implemented | `,h` then Camera on the Hold, picking or drawing any region target | [CAPTURED_FRAMING.md](CAPTURED_FRAMING.md), [TARGETS.md](TARGETS.md); replays `nested-pause`, `targets` | Face/region detection proposals; point targets | Partial: freeze in [`freeze-hold`](qualification/preview-export-2026-10-04.md); Camera on the Hold not exported |
| Black-frame punctuation | Implemented | `:hold 12f video=black` inserts a silent `HoldVideo::Background` pause at the cursor (one Undo); `:hold-duration` adjusts it | [INSERT_TIME.md](INSERT_TIME.md); `black_pause_inserts_background_picture_and_silence_as_one_undo` ([pause.rs](../crates/deadpan-app/src/project/tests/pause.rs)); replay `zoom` | Sound policy other than silence at insertion; `set_hold_provider` to black for an existing pause | [`black-pause`](qualification/preview-export-2026-10-04.md) |
| Delayed caption | Implemented | `:caption TEXT at=bottom\|top\|center delay=12f reveal=3` on the selected beat or Edit range; caption attachments on Source/Hold hosts drawn by the shared GPU pass in preview and export; `reveal=` waits for a Repeat play; recordable (`SetCaption`) | [CAPTIONS.md](CAPTIONS.md); `captions_show_in_their_host_range_from_their_reveal_play_without_changing_pictures`, `captions_composite_white_fill_and_dark_outline_before_encoder_readback`; replay `captions` (viewer pixel readback) | Captions over groups, wrapping and styling, complex-script shaping | [`delayed-caption`](qualification/preview-export-2026-10-04.md) (and a caption-free revision flags the captioned frame) |
| Abrupt return | Implemented | `:zoom off` removes framing on the beat, or steps out to the full picture over the Edit range (hard cut); also `s` split then Camera `r` | [FRAMING.md](FRAMING.md#zoom-and-creep-commands); [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)) | Step out of an existing camera path over a range (refused rather than flattened); export fixture | No |

## 8.3 Audio

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| Selective emphasis | Implemented | `:gain` editor envelopes (Step/Linear/Smoothstep/Cubic) in owner frames; `+`/`-` 3 dB trim | [AUDIO_GAIN.md](AUDIO_GAIN.md); replay `gain`; real export in replay `render` (trim only) | Sample-resolution ranges for clicks; envelopes over a Visual range across beats; point dragging | [`gain-trim`](qualification/preview-export-2026-10-04.md) (trim only) |
| Sudden silence | Implemented | `,h` silent Hold; `,m` mutes the Visual range inside the beat (mute range) or toggles the whole beat; gain editor mute ranges; `:gain-mute`; `:gag nothing-happens` cuts room tone to true silence | [AUDIO_GAIN.md](AUDIO_GAIN.md), [INSERT_TIME.md](INSERT_TIME.md); replays `gain`, `nested-pause`, `recipes` | Sample-resolution mute ranges | [`black-pause`](qualification/preview-export-2026-10-04.md) (silent Hold), [`mute-range`](qualification/preview-export-2026-10-04.md) (mute range, with sound outside it), [`nothing-happens`](qualification/preview-export-2026-10-04.md) (silence after room tone) |
| Room tone | Implemented | `:room-tone` on a selected Hold with exact source range and audition; `:hold-silence` reverts; `:gag nothing-happens register=r` takes it from a copied Original moment | [ROOM_TONE_AUDIO.md](ROOM_TONE_AUDIO.md), [GAGS.md](GAGS.md); `copied_moment_sources_yield_the_same_inward_audio_range`; replays `room-tone`, `recipes` | Repeat-gap and fragment controls; waveform display | [`nothing-happens`](qualification/preview-export-2026-10-04.md) (looped room tone, checked by its repeating click) |
| Hanging tail | Implemented | `HoldAudio::Tail` is a live reference: at render time it feeds the reverb or 300 ms echo with the processed Original sound heard over the 2 s before the pause in the current plan (edits, gain and mute before it change it; other tails do not feed it), ringing for its maximum then fading to exact silence (shared canonical preparation); `,t`, `:tail` change only a pause's sound | [REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md); `tail_hold_rings_the_processed_sound_heard_before_it_then_exact_silence`, `a_tail_follows_later_edits_to_the_sound_before_it`, `a_tail_does_not_feed_the_next_tail`, `gain_shared_by_the_tail_and_the_sound_before_it_applies_once`, `tails_in_repeats_splits_and_bound_clocks_render_and_the_original_after_them_reads`, `a_tail_cannot_be_wrapped_in_or_moved_into_a_speed_change`, `a_tail_on_a_pause_whose_picture_cannot_be_resolved_still_applies`; replay `hold-effects` | Sends from other beats or sounds; placed sounds in the tail's input; tails inside speed changes (invalid document); listening qualification | [`hanging-tail`](qualification/preview-export-2026-10-04.md) (reverb), [`tail-echo`](qualification/preview-export-2026-10-04.md) (echo) |
| Bleep | Implemented | `,b` / `:bleep 880Hz level=-6dB` over a Visual range (`viw` for a word): the range is cut into the register and refilled by a same-length Hold that plays the same pictures forward (`HoldVideo::Play`) over a synthesized tone (`HoldAudio::Tone`, 1 kHz at -10 dB by default, 2 ms ramps, exact integer phase); recordable (`Bleep`) | [REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md#bleeps); `bleep_resolves_the_range_pictures_before_cutting_and_refills_its_time`, `tone_hold_renders_the_canonical_sine_without_reading_media`; replay `hold-effects` | Overlay mode (tone mixed over the sound); bleeping across a cut or a framed group | [`bleep`](qualification/preview-export-2026-10-04.md) (pictures unchanged, the click replaced by the tone) |
| Audio lag | Implemented | `:audio-lag +80ms` / `-2f` / `0` sets the signed audio offset of the selected beat's Source (explicit link offset); the inspector shows it beside the link or as Sound offset | [SOURCE_AUDIO_MAPPING.md](SOURCE_AUDIO_MAPPING.md); replay `recipes` | Slip and Trim refuse a Source after its offset changes (the common edit window is cleared); key binding | [`audio-lag`](qualification/preview-export-2026-10-04.md) (click 2,400 samples later) |
| Premature sound (J-cut) | Missing | None | No role-only trim or audio-shift command | Role-only edge edits | No |
| Lingering sound (L-cut) | Missing | None | As above | Role-only edge edits | No |
| Saturation | Missing | None; `AudioTreatmentStage` has only `ClipGain` ([audio_gain.rs](../crates/deadpan-core/src/audio_gain.rs)) | | Saturation treatment stage and DSP | No |
| Pitch shift | Missing | None authored; the DSP adapter accepts `pitch_semitones` ([lib.rs](../native/deadpan-dsp/src/lib.rs)) | [AUDIO_DSP.md](AUDIO_DSP.md) | Authored semitone parameter, plan stage, command | No |
| Bed drop | Implemented | `,s` places a complete catalog sound; `:sound-cut` ends the selected sound abruptly at the Edit cursor (exact selection end, Hard edge); `:sound-edges hard` | [SOUND_EVENTS.md](SOUND_EVENTS.md); replays `sound-placement`, `sound-playback`, `recipes` | Cutting a sound that follows timeline cuts; beat-attached beds | [`sound-event`](qualification/preview-export-2026-10-04.md) (whole sound), [`bed-drop`](qualification/preview-export-2026-10-04.md) (the `:sound-cut` construction, `SoundEvent::cut_at`) |
| Wrongly triumphant sting | Partial | User-owned catalog sound placed with `,s`, `:sound-at`, `:sound-gain` | [SOUND_EVENTS.md](SOUND_EVENTS.md); replay `sound-placement` | Bundled original synthesized sting | [`sound-event`](qualification/preview-export-2026-10-04.md) |

## 8.4 Starter recipes

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| The Long Answer | Implemented | `:gag long-answer pause=1.5s creep=1.35` | [GAGS.md](GAGS.md); `the_long_answer_gag_is_one_editable_group_pinning_its_recipe`; replay `gags` | Live (AI) hold variant; recipe export fixture | No |
| One More Time | Implemented | `:gag one-more-time plays=3 gap=500ms shorten=200ms`: a Repeat with a silent freeze gap that is shorter after each play, grouped | [GAGS.md](GAGS.md); `one_more_time_repeats_with_each_gap_shorter_than_the_last`; replay `recipes` | Speed progression variant | [`one-more-time`](qualification/preview-export-2026-10-04.md) (recipe through the headless semantic path; each play's click checked) |
| Are We Done? | Implemented | `:gag are-we-done register=r pause=1.5s`: a freeze pause whose reverb tail rings through it under a reaction cutaway from the register's Original moment, grouped (`InsertPause`, `Tail`, `SetCutaway`) | [GAGS.md](GAGS.md), [REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md); `are_we_done_hangs_a_reverb_tail_under_a_reaction_cutaway_in_one_group` | Live (AI) hold variant | [`are-we-done`](qualification/preview-export-2026-10-04.md) |
| The Escalator | Implemented | `:gag escalator plays=3 gain-step=3dB zoom-step=0.08`, `,e` | [GAGS.md](GAGS.md), [REPEAT_ESCALATION.md](REPEAT_ESCALATION.md) | Recipe export fixture | No |
| The Non-Sequitur | Implemented | `:gag non-sequitur register=r` | [GAGS.md](GAGS.md) | Recipe export fixture | No |
| Nothing Happens | Implemented | `:gag nothing-happens register=r tone=1s silence=1s`: a held picture with room tone from the copied Original moment, then true silence, grouped | [GAGS.md](GAGS.md); `nothing_happens_holds_room_tone_then_true_silence_in_one_group`; replay `recipes` | Live (AI) hold variant | [`nothing-happens`](qualification/preview-export-2026-10-04.md) (recipe through the headless semantic path, room tone from a copied Original moment) |
| Versioned definition, pinned version and parameters | Implemented | Group label pins recipe, version, parameters; unavailable version refuses | [GAGS.md](GAGS.md) | Structured storage (label text only) | No |
| Exposed parameter editing after insertion | Partial | Parts are ordinary beats edited directly | [GAGS.md](GAGS.md) | Gag-aware inspector that edits recipe parameters | No |
| Save modified group as local recipe | Missing | None | [GAGS.md](GAGS.md#remaining) | Local recipe store and command | No |
| Inspect expansion | Missing | None | | Preview of expansion before apply | No |
| Detach from template | Implemented | Ungroup | [GROUP_EDITING.md](GROUP_EDITING.md); replay `groups` | | No |
| Seeded variation | Missing | None | | Stored seed and resolved values | No |

## §6.3 / §7.4 creative operators

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| Repeat operator | Implemented | `r` + object/motion, `rr`, `3riw`, Visual `r`, `:repeat N`, `:wrap-repeat N`, `:repeat N gap= gap-step=` | [REPEAT_SELECTION.md](REPEAT_SELECTION.md), [REPEAT_GAP_BRANCHES.md](REPEAT_GAP_BRANCHES.md); replays `repeat-operator`, `repeat-setters`, `recipes` | Editing one gap's content in place | [`repeat-with-gap`](qualification/preview-export-2026-10-04.md), [`repeat-gaps-steps`](qualification/preview-export-2026-10-04.md) |
| Hold (insert time) | Implemented | `,h`, `:hold` | [INSERT_TIME.md](INSERT_TIME.md) | Repeat/Retime and fractional-cursor insertion | [`freeze-hold`](qualification/preview-export-2026-10-04.md), [`black-pause`](qualification/preview-export-2026-10-04.md) |
| Replace picture | Implemented | `:cutaway` | [CUTAWAYS.md](CUTAWAYS.md) | See Reaction cutaway | [`cutaway`](qualification/preview-export-2026-10-04.md) |
| Retime | Implemented | `:retime`, `:wrap-retime` | [RETIME_EDITING.md](RETIME_EDITING.md) | See Stretch/pitch | [`retime-half`](qualification/preview-export-2026-10-04.md) |
| Group / ungroup | Implemented | `,g`, `:group name="…"`, `:ungroup` | [GROUP_EDITING.md](GROUP_EDITING.md); replay `groups` | | No |
| Paste from register | Implemented | `p`, `P`, `"x` prefix | [NAMED_REGISTERS.md](NAMED_REGISTERS.md) | | No |
| Dot-repeat of creative edits | Partial | `.` repeats cuts, Repeat wraps/setters and group edits | [SEMANTIC_REPEAT.md](SEMANTIC_REPEAT.md); replay `dot-repeat` | Framing, gain, cutaway, retime, Hold | No |
| Lift | Implemented | `:lift` cuts the Visual range into the register and refills its time with a silent black pause of exactly its length, so later content keeps its time; one Undo; recordable (`Lift`) | [REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md#lift); `lift_cuts_the_range_and_refills_its_time_with_a_silent_black_pause`; replay `hold-effects` | Lifting picture or sound alone (role-only) | [`lift`](qualification/preview-export-2026-10-04.md) |
| Role-only delete and `audio-shift` | Missing | None | | Audio-only/video-only delete, explicit audio ripple | No |
| Audio-only / video-only repeat (§6.5) | Missing | None | | Attached sample-timed repeat event with overflow policy | No |

## §7.5 comma-leader keys and `+`/`-`

Bindings are in [editor_map.rs](../crates/deadpan-app/src/navigation/editor_map.rs).

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| `,h` freeze hold | Implemented | Count scales 0.5 s | replays `keymap`, `gags`, `ai-pause` | | Construct: [`freeze-hold`](qualification/preview-export-2026-10-04.md) |
| `,i` reuse Original | Implemented | `:insert` alias | replay `workspace` ([scenarios.rs](../crates/deadpan-app/src/preview/harness/scenarios.rs)) | | No |
| `,s` place sound | Implemented | `:sound-place` alias | replay `sound-placement` | | No |
| `,a` AI hold | Implemented | Generates for a selected Hold | replay `ai-pause` | Distributed runtime; spec's insert-then-request in one key | No |
| `,z` punch in | Implemented | Follow of the selected target at 1.35x; center fallback with a message; Edit range honored | [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)) | Face proposals; recording ranged framing in macros | Construct: [`framing`](qualification/preview-export-2026-10-04.md) |
| `,c` creep | Implemented | Smoothstep to 1.35x over the beat or Edit range | [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)) | Target choice from the key (use `:creep target=`) | Construct: [`framing`](qualification/preview-export-2026-10-04.md) |
| `,m` mute | Implemented | Mutes the Visual range inside the selected beat as a mute range, or toggles the whole beat (`:gain-mute`); Your edit only, no count | [KEYMAP.md](KEYMAP.md); `comma_m_mutes_and_comma_r_picks_a_cutaway_in_your_edit_only`; replay `recipes` (Kestrel audit 10,890,672 cases) | Ranges across beats | Construct: [`mute-range`](qualification/preview-export-2026-10-04.md) |
| `,r` reaction picker | Implemented | Opens `:cutaway register=` on the selected (or first) register holding an Original moment and lists every such register with its range; Enter places it | [KEYMAP.md](KEYMAP.md), [CUTAWAYS.md](CUTAWAYS.md); `comma_m_mutes_and_comma_r_picks_a_cutaway_in_your_edit_only`; replay `recipes` | A visual thumbnail picker | Construct: [`cutaway`](qualification/preview-export-2026-10-04.md) |
| `,e` escalating repeat | Implemented | Three plays, +3 dB, +0.08 | replay `editing` | | Construct: [`escalating-repeat`](qualification/preview-export-2026-10-04.md) |
| `,b` bleep | Implemented | Bleeps the Visual range with the default tone; Your edit only, no count; `:bleep` sets frequency and level | [KEYMAP.md](KEYMAP.md); `comma_m_mutes_comma_r_picks_a_cutaway_and_comma_t_adds_a_tail_in_your_edit_only`, `bleeps_default_to_one_kilohertz_at_minus_ten_decibels`; replay `hold-effects` (Kestrel audit) | Overlay mode | Construct: [`bleep`](qualification/preview-export-2026-10-04.md) |
| `,t` reverb tail | Implemented | Opens `:tail` with the selected pause's length (or 1s) ready to change; Your edit only, no count | [KEYMAP.md](KEYMAP.md), [REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md); `comma_m_mutes_comma_r_picks_a_cutaway_and_comma_t_adds_a_tail_in_your_edit_only`; replay `hold-effects` (Kestrel audit 10,890,672 cases) | | Construct: [`hanging-tail`](qualification/preview-export-2026-10-04.md) |
| `,g` named group | Implemented | Prompts for a name | replay `groups` | | No |
| `,f` Camera | Implemented | | replays `camera`, `targets`, `scoped-plays` | | No |
| `,v` Trim | Implemented | | replay `trim` | | No |
| `+` / `-` gain | Partial | ±3 dB on the selected beat or placed sound | [AUDIO_GAIN.md](AUDIO_GAIN.md); replay `gain` | Visual-range scope | No |

## §6.4 command examples

Parser: [command.rs](../crates/deadpan-app/src/navigation/command.rs).

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| `:hold 1.5s video=freeze audio=silence` | Implemented | `video=freeze\|black`, `audio=silence` accepted | [INSERT_TIME.md](INSERT_TIME.md); `pause_command_keeps_exact_units_and_zero_without_committing` | `audio=room-tone` at insertion | Construct: [`freeze-hold`](qualification/preview-export-2026-10-04.md) |
| `:hold 1.5s video=ai audio=silence` | Partial | `:hold` then `,a` / `:generate` | [AI_HOLDS.md](AI_HOLDS.md) | One-command form | No |
| `:repeat 3 gap=120ms gain-step=3dB zoom-step=0.08` | Implemented | As written, on a selected Repeat or a plain beat (wrapped first); one recorded `SetRepeat` and one Undo; `gap-step=` adds a gap progression | [GAGS.md](GAGS.md#commands), [REPEAT_GAP_BRANCHES.md](REPEAT_GAP_BRANCHES.md); `gaps_parse_with_steps_and_resolve_each_length_once`; replays `editing`, `recipes` | | Construct: [`repeat-gaps-steps`](qualification/preview-export-2026-10-04.md) (the recorded `SetRepeat` through the headless semantic path) |
| `:zoom 1.35 target=face:2 curve=step` | Partial | `:zoom` with `target=current\|center\|ID or label`, `curve=step\|linear\|smoothstep`, `:zoom off` | [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)); `zoom_and_creep_commands_parse_through_the_command_line` | `face:N` needs detected face proposals | Construct: [`framing`](qualification/preview-export-2026-10-04.md) (center target) |
| `:creep from=1 to=1.4 target=current` | Implemented | As written | [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)) | Live target tracking during a creep | Construct: [`framing`](qualification/preview-export-2026-10-04.md) |
| `:gain +6dB` | Partial | `:gain 6` sets absolute trim; the `dB` suffix is refused by `parse_db` ([gain.rs](../crates/deadpan-app/src/gain.rs)) | | Unit suffix; relative form | No |
| `:retime 0.75 pitch=preserve` | Implemented | As written | replay `retime` | | Construct: [`retime-half`](qualification/preview-export-2026-10-04.md) (0.5) |
| `:cutaway register=r audio=keep` | Implemented | As written | replay `cutaway` | | Construct: [`cutaway`](qualification/preview-export-2026-10-04.md) |
| `:tail 400ms effect=reverb` | Implemented | As written, on a selected pause or as a new tail pause at the cursor; `effect=delay` echoes | [REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md); `tail_takes_an_optional_length_and_effect_in_any_order`; replay `hold-effects` | | Construct: [`hanging-tail`](qualification/preview-export-2026-10-04.md) (through the semantic `Tail` instruction) |
| `:trim edge=out delta=-3f mode=ripple` | Implemented | As written | replay `trim` | | No |
| `:slip +5f` | Implemented | As written | replay `slip` | | No |
| `:roll +2f` | Partial | Use `:trim edge=roll delta=+2f mode=…` | [COMBINED_TRIM.md](COMBINED_TRIM.md) | `:roll` verb | No |
| `:select role=audio` | Missing | `:select` takes no arguments | | Role selection | No |
| `:group name="the uncomfortable answer"` | Implemented | As written | replay `groups` | | No |
| `:render` | Implemented | As written | replay `render` | | Harness: [verify-export](PREVIEW_EXPORT_VERIFICATION.md) |

## Other Gate D items

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| Per-play overrides | Partial | `:scope play N` / `:scope all`; Gain, Camera and Hold audio isolate one play; `:repeat gap-step=` gives each later gap its own independent Hold | [SCOPED_EDITING.md](SCOPED_EDITING.md), [REPEAT_GAP_BRANCHES.md](REPEAT_GAP_BRANCHES.md); replays `scoped-plays`, `recipes` | Timing edits, copy/paste and macros inside plays; editing one gap's content natively | No |
| Tails | Partial | Hold tails (`HoldAudio::Tail`, reverb or echo) render with exact silence after the ring | [REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md) | Effect sends from arbitrary beats and placed sounds; tails crossing into non-Hold content | [`hanging-tail`](qualification/preview-export-2026-10-04.md), [`tail-echo`](qualification/preview-export-2026-10-04.md) |
| Stretch / pitch | Partial | Retime Preserve and FollowSpeed; reverse of an Original passage as a Hold (`:reverse`, `:ping-pong`) | [RETIME_EDITING.md](RETIME_EDITING.md), [AUDIO_STAGE_PREPARATION.md](AUDIO_STAGE_PREPARATION.md), [REVERSE_AND_TAILS.md](REVERSE_AND_TAILS.md) | Fixed pitch shift, variable rate, reverse of composite structure or at other speeds, long Preserve stages | Partial: [`retime-half`](qualification/preview-export-2026-10-04.md) (Preserve), [`reverse-hiccup`](qualification/preview-export-2026-10-04.md) (reverse) |
| Cutaways | Implemented | `:cutaway` | [CUTAWAYS.md](CUTAWAYS.md) | Multi-beat ranges, groups, per-play cutaways, timeline indication | [`cutaway`](qualification/preview-export-2026-10-04.md) |
| Framing | Partial | Camera, `,z`, `,c`, `:zoom`, `:creep`, targets, follow, tracking, escalation | [FRAMING.md](FRAMING.md), [TARGETS.md](TARGETS.md), [TRACKING.md](TRACKING.md) | Point targets, live creep on a moving target, detection, letterbox-corrected follow | Partial: [`framing`](qualification/preview-export-2026-10-04.md) |
| Saved gags | Partial | Six built-in versioned recipes via `:gag`, including Are We Done? | [GAGS.md](GAGS.md) | Save as local recipe, inspect expansion, seeded variation | Partial: [`one-more-time`](qualification/preview-export-2026-10-04.md), [`nothing-happens`](qualification/preview-export-2026-10-04.md), [`are-we-done`](qualification/preview-export-2026-10-04.md) (recipes) |
| Registers | Implemented | `"x`, `y`/`d`/`p`, `:register`, `:registers`; persisted bank | [NAMED_REGISTERS.md](NAMED_REGISTERS.md); replay `named-registers` | Cross-project transfer | No |
| Semantic macros | Partial | `q`/`@`, `:record`, `:macro a N`; one Compound transaction; records `:repeat` count, gap and step changes, every gag, reverses, tails, whole-beat cutaways and captions | [SEMANTIC_MACROS.md](SEMANTIC_MACROS.md); replays `macros`, `captions` | Recording of retime, gain, sound, room tone, Trim, ranged cutaways and captions, and scoped framing (`SemanticInstruction` in [program.rs](../crates/deadpan-core/src/semantic/program.rs)) | No |
| Shared command/help registry | Partial | Key help is derived from the binding trie; command help is a hand-written list in [preview.rs](../crates/deadpan-app/src/preview.rs) separate from the parser | [KEYMAP.md](KEYMAP.md) | Single command schema for parser, help, completion and palette; typed-unit grammar | No |
