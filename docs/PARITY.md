# Headless and GUI parity (DP-21)

This table lists every user-invocable action of the native app and its
headless equivalent in `deadpan-cli` (identical to `deadpan-app --headless`).
It was compiled from `Action`, `command::Entry` and the shipped key trie in
`crates/deadpan-app/src/navigation*`, the native menu in `menu.rs`, the modal
routers and their `ProjectRequest` dispatch, and checked against
`crates/deadpan-cli/src` (2026-10-05; parity gaps closed 2026-10-06). Each headless entry names an existing
subcommand, a `command` request kind (any [`Command`](../crates/deadpan-core/src/command.rs)
variant in snake_case), a semantic instruction run with `macro` `apply`, or a
live endpoint operation from [open projects](LIVE_PROJECT.md).
The internal `RestoreSnapshot` command requires a retained store proof;
callers restore an observed take through `project take`, not by supplying an
arbitrary document to `command`.

Status meanings:

- **Equivalent**: the headless path reaches the same core/store operation the
  GUI commits, with the same validation.
- **GUI-only**: navigation, view, audition, interactive preview, panel or
  personal UI state with no project write of its own; headless callers pass
  explicit context instead, and the write such a panel ends with is its own
  Equivalent row.

No action is Partial or a Gap: the registry's `Parity` type has only these two
statuses, so a new native action cannot ship without a headless form or a
reason it needs none.

Every headless write works on a closed project, on its own writer, and on a
project the app has open, through its authenticated
[live endpoint](LIVE_PROJECT.md), which runs the same shared executor on the
app's writer and refreshes the app. Native-derived requests (sound events, the
whole-Original registration, Original copies) are derived by the same shared
functions the app uses (`deadpan_cli::sound_events`,
`deadpan_cli::generation::variants`, `deadpan_cli::gags`) from a read-only
snapshot at the caller's expected revision, then committed through the
ordinary revision-checked path.

Checks column: **R** means an explicit expected revision (or, for analysis
corrections, an expected corrections version; for Macros also the register
bank version) refuses a stale request without writing. **D** means a dry run
validates through the same path and writes nothing. `-` means neither applies
or exists.

## How the GUI's edit paths map to headless requests

| GUI dispatch | Headless form |
|---|---|
| Semantic Apply (`ProjectRequest::Macro(Operation::Apply)`): operators, Visual edits, Repeat, Group, pauses, gags, captions, framing presets, gain steps and most `:` edits | `macro <p> --json` with `{"type":"apply","program":{"instructions":[...]},"parent","cursor","selected_child","visual_selection"}`. Same `plan_program` and store Compound as the native Apply. R (revision and bank version), D. |
| `ProjectRequest::Edit(ProjectEdit::*)` | `command <p> --json <request> [--dry-run]` with the matching core command. R, D. |
| `ProjectRequest::SoundEdit` | `sound <p> --json <request> [--dry-run]` derives the native event (`place`, `move`, `nudge`, `set`, `cut`, `allowance`, `delete`) and commits it as `set_sound`, `set_sound_allowance` or `delete_sound`; `command` takes complete events. R, D. |
| Operational project state (AI variants, corrections, storage, backups) | Live short operations `GenerationVariant`, `DismissInterruptedAttempt`, `Corrections`, `CleanStorage`, `ConfirmVariantClock`, `RestoreBackup`, through the same `execute_short` the closed commands use. Refreshes the app's inspector, transcript or retention status. |
| Undo / Redo | `project undo|redo <p> --expected <rev> [--dry-run]`. R, D. |
| `:takes`: list, save, update, rename, delete, open | `project takes <p>`; `project take <p> --json <request> [--dry-run]`. R (edit revision, catalog version and selected snapshot), D. Shared store catalog transactions; opening is one reversible edit. |
| Open project (writer held by the app) | Every write in this inventory routes to the app's authenticated endpoint (`Execute`, `Prepare`, `Render`, `Generate`) after a writer-lock conflict. Read-only reports and dry runs open their own reader. |

## Navigation, view and audition

All of these are **GUI-only**: they move cursors, panes or the heard position
and write nothing. Headless requests carry an explicit `parent`, `cursor`,
`selected_child` and `visual_selection` instead of GUI focus.

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `h` `l` `j` `k` `w` `b` `e` `W` `B` `]p` `[p` `]s` `[s` `gg` `G`, Home, End | `move_frames`, `move_beats`, `move_words`, `move_sentences`, `move_pauses`, `move_shots`, `move_scope` instructions inside an `apply` or saved Macro (they change only the reported context) | R, D | GUI-only (motions available as instructions) |
| Enter / `:enter`, Backspace / `:parent` | Pass the group as `parent` | - | GUI-only |
| `:scope all`, `:scope play N`, `]r` / `[r` (step plays) | `edit_occurrence` / `edit_scoped` carry the occurrence path | - | GUI-only |
| `/`, `n`, `N` (transcript search) | `transcript <p> --search <words>` | - | Equivalent (read) |
| `:source`, `:sequence`, View menu, Tab / Shift-Tab, Esc | none needed | - | GUI-only |
| Space / `:play`, Shift-Space / `:audition`, `:audition-context`, `:monitor P%` | `inspect-audio` reads exact PCM at a chosen stage | - | GUI-only |
| `?`, `:help`, `:registers`, Help menu | `macro inspect <p>` lists registers | - | GUI-only |
| `:diagnostics` | `doctor --project <p>` reports the CLI process's own counters and project state, not the app process's live counters | - | GUI-only |
| `'a` / `:jump a`, Ctrl-O / Ctrl-I, `:jump-back`, `:jump-forward`, `:marks` | `resolve-selection` resolves a named mark against a revision | - | GUI-only |
| `,` prefix hint, Inspector parameter buttons that only prefill `:` commands | none needed | - | GUI-only |

## Selection, registers and copy/paste

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `v`, `ig`/`ag`, speech objects `iw aw is as ip ap iS aS` | `begin_selection`, `finish_selection`, `clear_selection`, `select_object`, `select_speech` instructions, or `visual_selection` in the request | R, D | Equivalent |
| `:select role=audio|video|linked` | Role is a parameter of `delete_role` / `role_repeat` instructions | R, D | Equivalent |
| `"a`, `:register a` | Register named in each `yank`/`cut`/`paste` instruction | - | GUI-only |
| `yy`, `y` + motion/object, Visual `y` in Your edit | `yank`, `yank_beat`, `yank_selection` instructions | R, D | Equivalent |
| `y` in Original (copy a source moment, `CaptureOriginal`) | `macro <p> --json` with `{"type":"yank_original","register":"a","ordinals":{"start":S,"end":E}}` (optional `asset`): the same `RegisterValue::Original` into `"` and the named register, validated against the qualified index | R (revision and bank version), D | Equivalent |
| `p` / `P`, `:paste`, `:paste-before` (edited content) | `paste` / `replace_selection` instructions, or `splice_slice*` / `replace_slice*` commands | R, D | Equivalent |
| `p` / `P` of an Original moment (`PasteMoment`) | `yank_original`, then the `paste` / `replace_selection` instruction: the host derives the measured Source mapping from the register as the app does | R, D | Equivalent |
| `,i`, `:insert` (reuse whole Original) | `project insert-original <p> --parent <id> --index <N> --expected <rev> [--dry-run]`: the app's registration (same content, asset, label and qualified streams, fresh node) admitted through `register-source`'s path, which reuses the qualification; live `Prepare` when open | R, D | Equivalent |
| `:recipe-save a` (`yank` around group) / `:recipe a` (`paste`) | `yank` with `around_group` / `paste` instructions | R, D | Equivalent |
| `:recipe-inspect a` | `macro inspect <p> --register a` returns the edited register's `outline` | - | Equivalent (read) |

## Structural edits

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `dd`, `d` + motion/object, Visual `d` on objects | `cut`, `cut_selection` instructions | R, D | Equivalent |
| Visual `d` on a time range, `:delete` (`CutEditSlice` from Delete/DeleteRange) | `cut_selection` instruction, or `delete_ripple` / `delete_range` / `delete_children` commands (no register write) | R, D | Equivalent |
| `:delete role=audio|video`, Visual `d` with a role | `delete_role` instruction | R, D | Equivalent |
| `x`, `:delete-frames Nf` (`CutFrames`) | `cut_frames` instruction | R, D | Equivalent |
| `:lift` | `lift` instruction | R, D | Equivalent |
| `s`, `:split`, Inspector Split | `command` `split` | R, D | Equivalent |
| `,g`, `:group name=`, `:ungroup` | `group`, `ungroup` instructions; `group_selection`, `ungroup` commands | R, D | Equivalent |
| `:splice` panel and its Copy/Move/Replace toggles | `--dry-run` of the command the panel would commit previews the edit; Before/Proposed pictures and audition are interactive | D | GUI-only |
| Place slice Enter (commit) | `splice_slice`, `splice_slice_at`, `replace_slice`, `replace_slice_children`, `splice_source*`, `replace_source*`, `move_range` commands, or `paste` / `replace_selection` instructions | R, D | Equivalent |
| `.` (repeat last edit) | Resubmit the same `apply` instruction | R, D | Equivalent |

## Repeat and speed

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `rr`, `:wrap-repeat N`, Inspector Wrap repeat | `command` `wrap_repeat`, or `repeat` instruction | R, D | Equivalent |
| `r` + motion/object, Visual `r` | `repeat` instruction | R, D | Equivalent |
| `:repeat N` | `set_repeat_plays` instruction / `command` `set_repeat_plays` | R, D | Equivalent |
| `:repeat … gap= gap-step= gain-step= zoom-step= progression=`, `,e` | `set_repeat` instruction; `repeat` with escalation | R, D | Equivalent |
| `:repeat N role=audio|video` | `role_repeat` instruction | R, D | Equivalent |
| `:retime`, `:wrap-retime`, `:pitch` | `retime`, `pitch` instructions; `wrap_retime`, `set_retime` commands | R, D | Equivalent |
| `:slip ±Nf` panel | `command` `slip_source --dry-run` returns the exact resolution; the stopped-picture preview is interactive | D | GUI-only |
| Slip Apply | `command` `slip_source` | R, D | Equivalent |
| `,v`, `:trim`, `:roll` panel | `--dry-run` of `apply_source_trim`, `trim_source` or `roll_sources` returns the exact resolution; junction pictures, waveform and audition are interactive | D | GUI-only |
| Trim Apply | `command` `apply_source_trim` (or the scalar `trim_source` / `roll_sources`) | R, D | Equivalent |

## Holds, pauses and punctuation

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `,h`, `:hold D [video=] [audio=]`, Inspector Insert pause | `insert_pause` instruction (`command` `insert_time` is the lower-level form) | R, D | Equivalent |
| `:hold-duration Nf` | `set_hold_duration` instruction / command | R, D | Equivalent |
| `:hold-silence`, Inspector Use silence | `command` `set_hold_audio` (or `edit_scoped`) | R, D | Equivalent |
| `:room-tone` sheet (prepare and audition a range) | interactive; headless applies a range directly | - | GUI-only |
| Room tone Apply | `command` `set_hold_audio` with an exact RoomTone range, or the `set_room_tone` instruction from an Original register (`yank_original`) | R, D | Equivalent |
| `:reverse`, `:ping-pong` | `insert_reverse` instruction | R, D | Equivalent |
| `:tail`, `,t` | `tail` instruction | R, D | Equivalent |
| `:jcut`, `:lcut` | `split_edit` instruction | R, D | Equivalent |
| `,b`, `:bleep` | `bleep` instruction | R, D | Equivalent |

## Camera, framing and attention targets

| GUI | Headless | Checks | Status |
|---|---|---|---|
| Camera panel draft (`,f`, pan, scale, follow) | interactive; headless passes the final pose or envelope | D | GUI-only |
| Camera Enter (apply) | `command` `set_framing` (or `edit_scoped`) with an explicit pose or envelope | R, D | Equivalent |
| `,z`, `,c`, `:zoom`, `:creep` | `set_framing` instruction | R, D | Equivalent |
| `:zoom … target=face:N` | `detect-faces <p> --at <pts>`, `command` `set_target` with the chosen face, then the `set_framing` instruction naming it: the same detection, target and framing writes the app makes in sequence | R, D | Equivalent |
| `:framing-save a`, `@a` | `macro` `save` of a `set_framing` program, `macro` `run` | R, D | Equivalent |
| `:track`, Camera `T` | `track <p> … [--save <id>]` | R (head at start) | Equivalent |
| Camera `c` (correct region) | `track-correct <p> --target <id> --at <pts> --region <r>` | R (head at start) | Equivalent |
| Camera `n` (new region) | `command` `set_target` | R, D | Equivalent |
| `:track-cancel` | SIGINT to the running `track` process | - | GUI-only |

## Audio and gain

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `+` / `-`, `:gain ±dB`, `:gain +=dB`, `,m`, `:gain-mute`, `:saturate` | `set_audio` instruction (`trim`, `step`, `range_step`, `mute`, `saturation`) | R, D | Equivalent |
| Gain sheet draft (Before/Draft audition, waveform) | interactive | - | GUI-only |
| Gain sheet Apply, scoped gain | `command` `set_audio_treatments` / `edit_scoped` | R, D | Equivalent |
| `:edge hard|auto …` | `set_audio_edges` instruction; `set_audio_edge` command | R, D | Equivalent |
| `:audio-lag` | `set_audio_lag` instruction | R, D | Equivalent |
| `:proxies on|off|retry` | none (personal app setting, not project state) | - | GUI-only |

## Placed sounds

| GUI | Headless | Checks | Status |
|---|---|---|---|
| ⌘I Add Sound (`ImportSound`) | `project register-source` with `streams: {"type":"audio_only",...}` | R, D | Equivalent |
| `,s`, `:sound-place` | `sound` `{"type":"place","asset":…,"at":{"frame":N}}` (or `{"sample":N}`): the complete catalog span at that onset, derived by the shared `sound_events::placement` | R, D | Equivalent |
| `:sound-at`, `h`/`l` nudges | `sound` `move` (`at`) / `nudge` (`frames`) | R, D | Equivalent |
| `:sound-gain`, `+`/`-`, `:sound-edges` | `sound` `set` with `gain_millidecibels` or `gain_step_millidecibels`, and/or `edges` | R, D | Equivalent |
| `:sound-cut` | `sound` `cut` at an Edit frame | R, D | Equivalent |
| `:sound-allow`, `:sound-silence` | `sound` `allowance` at an Edit frame (resolves the one silent pause there), or `command` `set_sound_allowance` | R, D | Equivalent |
| `:sound-delete` | `sound` `delete`, or `command` `delete_sound` | R, D | Equivalent |
| `:sting` | `sound --write-sting <file>` writes the same synthesized bytes; then `project retain-original` and `register-source` with `audio_only` streams, the app's `ImportSound` steps | R, D | Equivalent |
| `:sounds` | none needed | - | GUI-only |

## Marks and history

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `ma`, `:mark a` | `command` `set_mark` | R, D | Equivalent |
| `:unmark a`, Marks panel Remove | `command` `delete_mark` | R, D | Equivalent |
| `u`, ⌘Z, `:undo`; ⌘⇧Z, Ctrl-R, `:redo`; Edit menu | `project undo|redo <p> --expected <rev> [--dry-run]`; live `History` | R, D | Equivalent |

## Macros

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `qa` … `q`, `:record a`, `:record-stop` | `macro <p> --json` `save` with an explicit program | R, D | Equivalent (no recording session headlessly) |
| `:record-cancel` | none needed | - | GUI-only |
| `@a`, `:macro a N` | `macro` `run` | R, D | Equivalent |
| Register display | `macro inspect <p> [--register a]` | - | Equivalent (read) |

## Captions, cutaways and gags

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `:caption TEXT …` | `set_caption` instruction | R, D | Equivalent |
| `:caption clear` | `command` `set_captions` | R, D | Equivalent |
| `,r`, `:cutaway …` | `set_cutaway` instruction | R, D | Equivalent |
| `:cutaway clear` | `command` `set_cutaways` | R, D | Equivalent |
| `:gag NAME k=v…` (built-in gags) | `gag` instruction with the recipe | R, D | Equivalent |
| `:gag-set k=v…` | `set_gag` instruction | R, D | Equivalent |
| `:gag-inspect …` | `gag-inspect <p> --json <recipe> [--visual]`: the same step lines (shared `gags::expansion_rows`) and the expanded instructions at the project's rate | - | Equivalent (read) |
| `:gag PRESET`, `:gag-save NAME`, `:gag-presets` | none: presets live in the user's Application Support library, which headless paths never read; pass the recipe explicitly | - | GUI-only by design |

## Analysis and corrections

| GUI | Headless | Checks | Status |
|---|---|---|---|
| Transcribe, detect pauses (background saves) | `transcribe`, `pauses`, `transcript` | - | Equivalent |
| Shot detection | `detect-shots`, `shots` | - | Equivalent |
| `:correct` Enter/`c` (edit text), `x` (remove word/pause), `J` (join), `p` (pause after word), edge nudges, `D` (discard unreadable / drop inapplicable), `u` / `U` | `corrections <p> --json <request> [--dry-run]` with `edit_word`, `remove_word`, `join_words`, `set_word_bounds`, `add_pause_after`, `remove_pause`, `set_pause_bounds`, `drop_inapplicable`, `discard_unreadable`, `undo`, `redo`; `corrections <p>` inspects. Open projects: live `Corrections`, which republishes the app's corrected transcript and pauses | R (version and seen target), D | Equivalent |

## AI pauses

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `,a`, `3,a`, `:ai-hold DURATION` | semantic `insert_ai_pause` via macro apply; typed `insert_ai_time` | R, D | Equivalent |
| `:hold-provider ai`, `:generate N`, Generate another | `generate-hold <p> --hold <id> [--variants N] [--another]`; live `Generate` | - | Equivalent |
| `:revert-ai`, `:hold-provider fallback` | `revert_generated_hold`; `edit_scoped` with `revert_generated_hold` for Default/Play | R, D | Equivalent |
| Jobs Retry/Discard an AI preparation | `ai-replacements <p> --retry ID --expected REVISION` / `--discard ID --sequence N`; closed projects run queued work with `--run` | R | Equivalent |
| `:cancel-ai` | SIGINT / live `CancelGeneration` | - | Equivalent |
| Inspector variant list | `ai-variants <p> [--hold <id>]`: the shared offered-variant set, selection, kept/picked state, expiry and interrupted attempts | - | Equivalent (read) |
| `:next-ai`, `:prev-ai`, `:pick-ai N` | `select-hold <p> --request <id> --attempt <id>`; live `GenerationVariant` | - | Equivalent |
| `:accept-ai` | `accept-hold <p> --request <id> [--attempt <id>]`; live `AcceptHold` | R (live only) | Equivalent |
| `:discard-ai` | `discard-hold <p> --request <id> --attempt <id>` | - | Equivalent |
| `:keep-ai` | `keep-hold <p> --request <id> --attempt <id> [--off]` | - | Equivalent |
| Jobs `d` (discard an interrupted attempt) | `dismiss-attempt <p> --request <id> --attempt <id>` | - | Equivalent |
| `:compare-ai`, `,x`, `,n` | interactive Before/variant switching; `ai-variants <p> --joins` reports the same advisory join readings, decoded read-only | - | GUI-only |
| `:preview-ai`, `:audition-ai` | none needed | - | GUI-only |

## Render and export

| GUI | Headless | Checks | Status |
|---|---|---|---|
| ⌘E, `:render`, Render… | `render <p> --output <dir> [--name] [--expected <rev>]`; live `Render` | R | Equivalent |
| Render Cancel | SIGINT / live `Render` cancel | - | Equivalent |
| `:renders`, Saved Renders… | `render status <p> …` | - | Equivalent (read) |
| Recover from saved renders | `render retry`, `render reencode`, `render reconcile` | - | Equivalent |
| Commit-and-render with an open Camera/Gain/Room tone preview | none needed (headless has no temporary previews; live Render refuses while one is open) | - | GUI-only |

## Project and files

| GUI | Headless | Checks | Status |
|---|---|---|---|
| ⌘N New Project (from a video) | `project create-original <p> <video>` | - | Equivalent |
| ⌘⇧N, `:youtube`, `:new-url` | `project create-from-url`, `downloader install|status` | - | Equivalent |
| ⌘O Open, Close Project, `:close` | none needed | - | GUI-only |
| ⌘I Choose Original / Import Media, `:sound-channels mono\|stereo\|none` | `project retain-original`, `project register-source`; live `Prepare` | R, D | Equivalent |
| `:relink`, Recovery Locate…; moved linked files relinked on open | `project relink-original … --expected-version <N>`, `project relink-moved <p>` | R | Equivalent |
| `:recovery` report, Acknowledge | the report and its acknowledgement describe the app's own open session and write nothing; `project validate`, `project originals`, `verify-original`, `render status` and `ai-variants` read the same durable facts | - | GUI-only |
| `:backups`, Storage B/J/K/O: back up, choose, restore | `project backup <p>`, `project backups <p> [--verify]`, `project restore <p> <id> [--expected <rev>] [--dry-run]` ([backups](BACKUPS.md)); open projects restore on the app's writer through live `RestoreBackup`, which starts the app's new session and replies before rebinding its endpoint | R, D | Equivalent |
| `:storage`, Storage… report; C cache cleanup; S, `:portable-copy`, File › Save Portable Copy… | `project storage <p> [--grace-hours N]`, `cache status|clean [--dry-run]`, `project copy-portable <p> <dest>` ([storage](STORAGE.md)) | D | Equivalent |
| Storage P (preview) | `project storage <p> --clean --files-only --dry-run`: the same read-only preview, plus a `plan` (project, head revision, grace, each file's name, device and inode) and its `plan_hash` | D | Equivalent |
| Storage R (remove the previewed files) | `project storage <p> --clean --files-only --plan <dry-run.json>`: `clean_previewed_storage` removes exactly the planned files a fresh reference scan still finds removable, as R does; a plan for another head is refused (`StoragePlanStale`). Open projects: live `CleanStorage` with the plan, refused like R while a job could publish media | R (plan revision), D | Equivalent |
| (headless convenience, no native counterpart) | `project storage <p> --clean [--files-only]` without a plan expires due AI variants (unless `--files-only`) and removes whatever a fresh scan finds unreferenced at that moment; on an open project it plans and scans off the app's service thread and applies only the rechecked writes | D | not an inventory row |
| Storage E (confirm the clock for AI variant expiry) | `project storage <p> --confirm-clock [--dry-run]`; live `ConfirmVariantClock`. A clock behind the project's records is refused, as in the app | D | Equivalent |

## Models

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `:models`, Models… panel Install/Resume, from folder/archive, Remove, Discard partial, Cancel | `models list|license|install|import|export|remove [--partial]`, SIGINT | - | Equivalent |

## Counts

Counting each row once (118 rows): Equivalent 92, GUI-only 26, Partial 0,
Gap 0. Navigation rows whose motions also exist as instructions count as
GUI-only. The [command reference](COMMANDS.md) carries the same status per
registry action.

## What stays GUI-only, and why

- **Navigation, view and focus**: they move cursors, panes or scope and write
  nothing; every headless request names its `parent`, `cursor`,
  `selected_child` and `visual_selection` explicitly.
- **Audition and monitoring**: hearing is not reproducible headlessly;
  `inspect-audio` reads the exact PCM at any processing stage instead.
- **Interactive drafts and panels** (Trim, Slip, Place slice, Camera, Gain,
  Room tone, AI preview and compare): their pictures, waveforms and audition
  respond to a person. Each panel ends in one write, which is an Equivalent
  row, and each such command has a `--dry-run` returning the exact proposal.
- **Session reports** (`:recovery`, `:diagnostics`, `:jobs`): they describe
  the app process's own open session; the CLI reports the durable facts and its
  own counters.
- **Personal settings** (`:proxies`, saved gag presets, keymaps): they live in
  the user's Application Support library, which headless paths never read; pass
  the recipe or setting explicitly.

Live audition, the interactive previews and these reports are the only
remaining differences. Complete recovery/failure acceptance and the broader
command/selector surface of DP-21 are tracked in
[REQUIREMENTS](REQUIREMENTS.md).
