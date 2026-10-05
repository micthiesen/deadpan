# Section 8 coverage inventory

Date: 2026-10-04. This is a living inventory for Gate D in
[Requirements](REQUIREMENTS.md): every Section 8 operation and starter recipe,
per-play overrides, tails, stretch/pitch, cutaways, framing, saved gags,
registers, semantic macros and the shared command/help registry. Update a row
in the same change that alters its status.

**Summary: 47 implemented, 23 partial, 25 missing (95 rows).** Rows overlap on
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
| Frozen stare | Implemented | `,h` freeze of the measured picture; Camera (`,f`) on the Hold; `:room-tone` / `:hold-silence` for audio | [CAPTURED_FRAMING.md](CAPTURED_FRAMING.md), [ROOM_TONE_AUDIO.md](ROOM_TONE_AUDIO.md); replays `nested-pause`, `room-tone`, `camera` | Export of Camera on the Hold and of room tone | [`freeze-hold`](qualification/preview-export-2026-10-04.md) (freeze and silence; Camera on the Hold and room tone not exported) |
| Living stare | Partial | `,a` / `:generate`, `:preview-ai`, `:accept-ai` on a Hold; acceptance leaves Hold audio unchanged | [AI_HOLDS.md](AI_HOLDS.md), [GENERATED_HOLDS.md](GENERATED_HOLDS.md); replay `ai-pause` | Distributed model runtime (generation needs the development MLX harness); candidate sound audition; scoped Repeat/Retime entry | No |
| Micro-loop | Partial | Visual range + `r` repeats a short fragment; or `:cutaway fit=loop` loops a moment over a Hold | [REPEAT_SELECTION.md](REPEAT_SELECTION.md), [CUTAWAYS.md](CUTAWAYS.md); replays `repeat-operator`, `cutaway` | Explicit seam treatment in the app (`set_audio_edge` is headless only); a Repeat nested inside a Hold | [`repeat-with-gap`](qualification/preview-export-2026-10-04.md) (linked Repeat) |
| Ping-pong hold | Missing | None | No reverse primitive in `NodeKind` ([document.rs](../crates/deadpan-core/src/document.rs)) | Reverse playback node or mapping, endpoint deduplication | No |
| Word / syllable stutter | Partial | `r` + `iw` / motion, `3riw`, Visual `r` (linked) | [REPEAT_SELECTION.md](REPEAT_SELECTION.md); replays `repeat-operator`, `transcript` | Audio-only repeat (§6.5); native inter-play gap (`:repeat gap=` refused; `set_repeat` / `set_gap_override` headless only) | [`repeat-with-gap`](qualification/preview-export-2026-10-04.md) (linked Repeat with gap) |
| Escalation | Partial | `,e`; `:repeat 3 gain-step=3dB zoom-step=0.08 [progression=multiply]` on a Repeat | [REPEAT_ESCALATION.md](REPEAT_ESCALATION.md); `escalation_scales_each_later_play_and_its_gap_inside_the_repeat_framing` ([picture_plan.rs](../crates/deadpan-plan/tests/picture_plan.rs)), `repeat_escalation_adds_its_step_to_each_later_play_and_its_gap` ([gain.rs](../crates/deadpan-audio/tests/stages/gain.rs)); replay `editing` | Speed and gap progression; target-centered zoom; count and steps in one command | [`escalating-repeat`](qualification/preview-export-2026-10-04.md) |
| False start | Implemented (composed) | Original `v`/`y` short prefix, `p`, `,h`, then `v`/`y`/`p` a longer span | [SOURCE_MOMENTS.md](SOURCE_MOMENTS.md), [INSERT_TIME.md](INSERT_TIME.md); replays `original-moment`, `nested-pause` | Dedicated recipe; export fixture | No |
| Interrupted answer | Implemented (composed) | `s` split, then `,h` or `:cutaway`, and `d`/`dd` to omit the ending | [STRUCTURAL_SPLIT.md](STRUCTURAL_SPLIT.md), [CUTAWAYS.md](CUTAWAYS.md); replays `delete-range`, `cutaway` | Dedicated recipe; export fixture | No |
| Callback | Implemented | `"a` + `y`/`d`, later `"a` + `p`/`P`; `,g` names the group | [NAMED_REGISTERS.md](NAMED_REGISTERS.md), [EDITED_SLICES.md](EDITED_SLICES.md); replays `named-registers`, `place-slice` | Named-range (mark range) paste selector | No |
| Reverse hiccup | Missing | None | No reverse primitive; DSP adapter has none either | Reverse picture and audio mapping | No |
| Slow delivery | Implemented | `:retime 0.75 pitch=preserve` / `pitch=tape`, `:wrap-retime`, inspector Change speed | [RETIME_EDITING.md](RETIME_EDITING.md); replay `retime` | Preserve stage limited to about 21.8 s input; variable rate | [`retime-half`](qualification/preview-export-2026-10-04.md) (50%, Preserve) |

## 8.2 Attention and picture

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| Smash zoom | Partial | `,z` (1.35x step that follows the selected target, or at the current center when none is saved); `:zoom S target=current\|center\|ID curve=step`, optionally over the Edit range inside the beat | [FRAMING.md](FRAMING.md#zoom-and-creep-commands); [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)) | Face/region proposals for `target=face:N` | [`framing`](qualification/preview-export-2026-10-04.md) (static 1.35x) |
| Slow creep | Implemented | `,c` (to 1.35x over the beat or the Edit range inside it); `:creep from= to= target= curve=smoothstep\|linear`; `:zoom S curve=linear`; gag `long-answer creep=` | [FRAMING.md](FRAMING.md#zoom-and-creep-commands), [GAGS.md](GAGS.md); [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)); `the_long_answer_inserts_a_pause_and_creeps_on_it_in_one_transaction` | Creep that keeps following a moving target (live follow with a scale envelope); creep over an entire Repeat from outside it | [`framing`](qualification/preview-export-2026-10-04.md) (smoothstep creep) |
| Escalating crop | Implemented | `zoom-step=` on a Repeat (centered, additive or multiplicative) | [REPEAT_ESCALATION.md](REPEAT_ESCALATION.md); `escalation_scales_each_later_play_and_its_gap_inside_the_repeat_framing`; replay `editing` | Target-centered or center-moving increments | [`escalating-repeat`](qualification/preview-export-2026-10-04.md) |
| Reaction cutaway | Implemented | `:cutaway register=r fit=hold\|loop\|gap audio=keep`, `:cutaway clear` | [CUTAWAYS.md](CUTAWAYS.md); `a_cutaway_replaces_the_host_picture_in_its_range_and_holds_its_last_picture`, `a_cutaway_survives_a_split_through_its_range_with_every_picture_unchanged` ([picture_plan.rs](../crates/deadpan-plan/tests/picture_plan.rs)); replay `cutaway` | `,r` picker; ranges across beats; cutaways over groups or one Repeat play | [`cutaway`](qualification/preview-export-2026-10-04.md) (Hold fit) |
| Reaction ping-pong | Implemented (composed) | Several disjoint `:cutaway` ranges on one host (sound kept), or alternating pasted Original moments (sound switched) | [CUTAWAYS.md](CUTAWAYS.md), [SOURCE_MOMENTS.md](SOURCE_MOMENTS.md) | Single alternation command; export fixture | No |
| Off-center stare | Partial | Camera `h/j/k/l`, `+`/`-`; draw a target with `n` and follow it with `t` | [TARGETS.md](TARGETS.md), [FRAMING.md](FRAMING.md); replay `targets` | Reusable named framing preset; target rename/delete in app; targets end at the next shot boundary | [`framing`](qualification/preview-export-2026-10-04.md) (follow of a moving target) |
| Freeze a detail | Implemented | `,h` then Camera on the Hold, picking or drawing any region target | [CAPTURED_FRAMING.md](CAPTURED_FRAMING.md), [TARGETS.md](TARGETS.md); replays `nested-pause`, `targets` | Face/region detection proposals; point targets | Partial: freeze in [`freeze-hold`](qualification/preview-export-2026-10-04.md); Camera on the Hold not exported |
| Black-frame punctuation | Implemented | `:hold 12f video=black` inserts a silent `HoldVideo::Background` pause at the cursor (one Undo); `:hold-duration` adjusts it | [INSERT_TIME.md](INSERT_TIME.md); `black_pause_inserts_background_picture_and_silence_as_one_undo` ([pause.rs](../crates/deadpan-app/src/project/tests/pause.rs)); replay `zoom` | Sound policy other than silence at insertion; `set_hold_provider` to black for an existing pause | [`black-pause`](qualification/preview-export-2026-10-04.md) |
| Delayed caption | Missing | None | No text attachment in `BeatNode` | Text attachment model, rendering, per-play reveal | No |
| Abrupt return | Implemented | `:zoom off` removes framing on the beat, or steps out to the full picture over the Edit range (hard cut); also `s` split then Camera `r` | [FRAMING.md](FRAMING.md#zoom-and-creep-commands); [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)) | Step out of an existing camera path over a range (refused rather than flattened); export fixture | No |

## 8.3 Audio

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| Selective emphasis | Implemented | `:gain` editor envelopes (Step/Linear/Smoothstep/Cubic) in owner frames; `+`/`-` 3 dB trim | [AUDIO_GAIN.md](AUDIO_GAIN.md); replay `gain`; real export in replay `render` (trim only) | Sample-resolution ranges for clicks; envelopes over a Visual range across beats; point dragging | [`gain-trim`](qualification/preview-export-2026-10-04.md) (trim only) |
| Sudden silence | Implemented | `,h` silent Hold; gain editor mute ranges; `:gain-mute` whole-beat mute | [AUDIO_GAIN.md](AUDIO_GAIN.md), [INSERT_TIME.md](INSERT_TIME.md); replays `gain`, `nested-pause` | `,m` | [`black-pause`](qualification/preview-export-2026-10-04.md) (silent Hold only) |
| Room tone | Implemented | `:room-tone` on a selected Hold with exact source range and audition; `:hold-silence` reverts | [ROOM_TONE_AUDIO.md](ROOM_TONE_AUDIO.md); replay `room-tone` | Repeat-gap and fragment controls; waveform display; export fixture | No |
| Hanging tail | Missing | `HoldAudio::Tail` is vocabulary only; the renderer rejects tail processing | [ROOM_TONE_AUDIO.md](ROOM_TONE_AUDIO.md) | Delay/reverb sends, tail policy rendering, `:tail`, `,t` | No |
| Bleep | Missing | None | No tone generator in core, plan or audio | Synthesized tone source, replace/overlay, `,b` | No |
| Audio lag | Partial | `SourceNode` signed audio offset via headless `set_source_audio_mapping` | [SOURCE_AUDIO_MAPPING.md](SOURCE_AUDIO_MAPPING.md), [HEADLESS.md](HEADLESS.md) | Native command or key; visible link offset | No |
| Premature sound (J-cut) | Missing | None | No role-only trim or audio-shift command | Role-only edge edits | No |
| Lingering sound (L-cut) | Missing | None | As above | Role-only edge edits | No |
| Saturation | Missing | None; `AudioTreatmentStage` has only `ClipGain` ([audio_gain.rs](../crates/deadpan-core/src/audio_gain.rs)) | | Saturation treatment stage and DSP | No |
| Pitch shift | Missing | None authored; the DSP adapter accepts `pitch_semitones` ([lib.rs](../native/deadpan-dsp/src/lib.rs)) | [AUDIO_DSP.md](AUDIO_DSP.md) | Authored semitone parameter, plan stage, command | No |
| Bed drop | Partial | `,s` places a complete catalog sound; `:sound-edges hard`; exact selected interval only through headless `set_sound` | [SOUND_EVENTS.md](SOUND_EVENTS.md); replays `sound-placement`, `sound-playback` | Native trim of a placed sound at an anchor | [`sound-event`](qualification/preview-export-2026-10-04.md) (whole sound) |
| Wrongly triumphant sting | Partial | User-owned catalog sound placed with `,s`, `:sound-at`, `:sound-gain` | [SOUND_EVENTS.md](SOUND_EVENTS.md); replay `sound-placement` | Bundled original synthesized sting | [`sound-event`](qualification/preview-export-2026-10-04.md) |

## 8.4 Starter recipes

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| The Long Answer | Implemented | `:gag long-answer pause=1.5s creep=1.35` | [GAGS.md](GAGS.md); `the_long_answer_gag_is_one_editable_group_pinning_its_recipe`; replay `gags` | Live (AI) hold variant; recipe export fixture | No |
| One More Time | Missing | None | [GAGS.md](GAGS.md#remaining) | Gap progression | No |
| Are We Done? | Missing | None | | Recipe; depends on tails | No |
| The Escalator | Implemented | `:gag escalator plays=3 gain-step=3dB zoom-step=0.08`, `,e` | [GAGS.md](GAGS.md), [REPEAT_ESCALATION.md](REPEAT_ESCALATION.md) | Recipe export fixture | No |
| The Non-Sequitur | Implemented | `:gag non-sequitur register=r` | [GAGS.md](GAGS.md) | Recipe export fixture | No |
| Nothing Happens | Missing | None (two Holds with room tone then silence can be composed by hand) | | Recipe | No |
| Versioned definition, pinned version and parameters | Implemented | Group label pins recipe, version, parameters; unavailable version refuses | [GAGS.md](GAGS.md) | Structured storage (label text only) | No |
| Exposed parameter editing after insertion | Partial | Parts are ordinary beats edited directly | [GAGS.md](GAGS.md) | Gag-aware inspector that edits recipe parameters | No |
| Save modified group as local recipe | Missing | None | [GAGS.md](GAGS.md#remaining) | Local recipe store and command | No |
| Inspect expansion | Missing | None | | Preview of expansion before apply | No |
| Detach from template | Implemented | Ungroup | [GROUP_EDITING.md](GROUP_EDITING.md); replay `groups` | | No |
| Seeded variation | Missing | None | | Stored seed and resolved values | No |

## §6.3 / §7.4 creative operators

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| Repeat operator | Implemented | `r` + object/motion, `rr`, `3riw`, Visual `r`, `:repeat N`, `:wrap-repeat N` | [REPEAT_SELECTION.md](REPEAT_SELECTION.md); replays `repeat-operator`, `repeat-setters` | Native gap parameters | [`repeat-with-gap`](qualification/preview-export-2026-10-04.md) |
| Hold (insert time) | Implemented | `,h`, `:hold` | [INSERT_TIME.md](INSERT_TIME.md) | Repeat/Retime and fractional-cursor insertion | [`freeze-hold`](qualification/preview-export-2026-10-04.md), [`black-pause`](qualification/preview-export-2026-10-04.md) |
| Replace picture | Implemented | `:cutaway` | [CUTAWAYS.md](CUTAWAYS.md) | See Reaction cutaway | [`cutaway`](qualification/preview-export-2026-10-04.md) |
| Retime | Implemented | `:retime`, `:wrap-retime` | [RETIME_EDITING.md](RETIME_EDITING.md) | See Stretch/pitch | [`retime-half`](qualification/preview-export-2026-10-04.md) |
| Group / ungroup | Implemented | `,g`, `:group name="…"`, `:ungroup` | [GROUP_EDITING.md](GROUP_EDITING.md); replay `groups` | | No |
| Paste from register | Implemented | `p`, `P`, `"x` prefix | [NAMED_REGISTERS.md](NAMED_REGISTERS.md) | | No |
| Dot-repeat of creative edits | Partial | `.` repeats cuts, Repeat wraps/setters and group edits | [SEMANTIC_REPEAT.md](SEMANTIC_REPEAT.md); replay `dot-repeat` | Framing, gain, cutaway, retime, Hold | No |
| Lift | Missing | None | | Same-duration blank/silent replacement | No |
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
| `,m` mute | Missing | Not bound; `:gain-mute` toggles whole-beat mute | [command.rs](../crates/deadpan-app/src/navigation/command.rs) | Key binding; selected-range mute | No |
| `,r` reaction picker | Missing | Not bound; `:cutaway register=r` exists | | Register picker | No |
| `,e` escalating repeat | Implemented | Three plays, +3 dB, +0.08 | replay `editing` | | Construct: [`escalating-repeat`](qualification/preview-export-2026-10-04.md) |
| `,b` bleep | Missing | Not bound | | Tone generator | No |
| `,t` reverb tail | Missing | Not bound | | Tail rendering | No |
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
| `:repeat 3 gap=120ms gain-step=3dB zoom-step=0.08` | Partial | Steps work on an existing Repeat; count must match | [REPEAT_ESCALATION.md](REPEAT_ESCALATION.md) | `gap=`; count change with steps | No |
| `:zoom 1.35 target=face:2 curve=step` | Partial | `:zoom` with `target=current\|center\|ID or label`, `curve=step\|linear\|smoothstep`, `:zoom off` | [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)); `zoom_and_creep_commands_parse_through_the_command_line` | `face:N` needs detected face proposals | Construct: [`framing`](qualification/preview-export-2026-10-04.md) (center target) |
| `:creep from=1 to=1.4 target=current` | Implemented | As written | [zoom.rs](../crates/deadpan-app/src/navigation/zoom.rs) unit tests; replay `zoom` ([zoom.rs](../crates/deadpan-app/src/preview/harness/zoom.rs)) | Live target tracking during a creep | Construct: [`framing`](qualification/preview-export-2026-10-04.md) |
| `:gain +6dB` | Partial | `:gain 6` sets absolute trim; the `dB` suffix is refused by `parse_db` ([gain.rs](../crates/deadpan-app/src/gain.rs)) | | Unit suffix; relative form | No |
| `:retime 0.75 pitch=preserve` | Implemented | As written | replay `retime` | | Construct: [`retime-half`](qualification/preview-export-2026-10-04.md) (0.5) |
| `:cutaway register=r audio=keep` | Implemented | As written | replay `cutaway` | | Construct: [`cutaway`](qualification/preview-export-2026-10-04.md) |
| `:tail 400ms effect=reverb` | Missing | No verb | | Tail rendering | No |
| `:trim edge=out delta=-3f mode=ripple` | Implemented | As written | replay `trim` | | No |
| `:slip +5f` | Implemented | As written | replay `slip` | | No |
| `:roll +2f` | Partial | Use `:trim edge=roll delta=+2f mode=…` | [COMBINED_TRIM.md](COMBINED_TRIM.md) | `:roll` verb | No |
| `:select role=audio` | Missing | `:select` takes no arguments | | Role selection | No |
| `:group name="the uncomfortable answer"` | Implemented | As written | replay `groups` | | No |
| `:render` | Implemented | As written | replay `render` | | Harness: [verify-export](PREVIEW_EXPORT_VERIFICATION.md) |

## Other Gate D items

| Operation | Status | Construction and access | Evidence | Missing | Export verified |
| --- | --- | --- | --- | --- | --- |
| Per-play overrides | Partial | `:scope play N` / `:scope all`; Gain, Camera and Hold audio isolate one play | [SCOPED_EDITING.md](SCOPED_EDITING.md), [REPEAT_GAP_BRANCHES.md](REPEAT_GAP_BRANCHES.md); replay `scoped-plays` | Timing edits, copy/paste and macros inside plays; native gap overrides (`set_gap_override` headless only) | No |
| Tails | Missing | `HoldAudio::Tail` vocabulary only | [ROOM_TONE_AUDIO.md](ROOM_TONE_AUDIO.md) | Sends, delay/reverb, tail rendering | No |
| Stretch / pitch | Partial | Retime Preserve and FollowSpeed | [RETIME_EDITING.md](RETIME_EDITING.md), [AUDIO_STAGE_PREPARATION.md](AUDIO_STAGE_PREPARATION.md) | Fixed pitch shift, reverse, variable rate, long Preserve stages | Partial: [`retime-half`](qualification/preview-export-2026-10-04.md) (Preserve) |
| Cutaways | Implemented | `:cutaway` | [CUTAWAYS.md](CUTAWAYS.md) | Multi-beat ranges, groups, per-play cutaways, timeline indication | [`cutaway`](qualification/preview-export-2026-10-04.md) |
| Framing | Partial | Camera, `,z`, `,c`, `:zoom`, `:creep`, targets, follow, tracking, escalation | [FRAMING.md](FRAMING.md), [TARGETS.md](TARGETS.md), [TRACKING.md](TRACKING.md) | Point targets, live creep on a moving target, detection, letterbox-corrected follow | Partial: [`framing`](qualification/preview-export-2026-10-04.md) |
| Saved gags | Partial | Three built-in versioned recipes via `:gag` | [GAGS.md](GAGS.md) | Three spec recipes, save as local recipe, inspect expansion, seeded variation | No |
| Registers | Implemented | `"x`, `y`/`d`/`p`, `:register`, `:registers`; persisted bank | [NAMED_REGISTERS.md](NAMED_REGISTERS.md); replay `named-registers` | Cross-project transfer | No |
| Semantic macros | Partial | `q`/`@`, `:record`, `:macro a N`; one Compound transaction | [SEMANTIC_MACROS.md](SEMANTIC_MACROS.md); replay `macros` | Recording of retime, cutaway, gain, sound, room tone, Trim, scoped framing and `:repeat` steps (`SemanticInstruction` in [program.rs](../crates/deadpan-core/src/semantic/program.rs)) | No |
| Shared command/help registry | Partial | Key help is derived from the binding trie; command help is a hand-written list in [preview.rs](../crates/deadpan-app/src/preview.rs) separate from the parser | [KEYMAP.md](KEYMAP.md) | Single command schema for parser, help, completion and palette; typed-unit grammar | No |
