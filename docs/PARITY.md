# Headless and GUI parity (DP-21)

This table lists every user-invocable action of the native app and its
headless equivalent in `deadpan-cli` (identical to `deadpan-app --headless`).
It was compiled from `Action`, `command::Entry` and the shipped key trie in
`crates/deadpan-app/src/navigation*`, the native menu in `menu.rs`, the modal
routers and their `ProjectRequest` dispatch, and checked against
`crates/deadpan-cli/src` (2026-10-05). Each headless entry names an existing
subcommand, a `command` request kind (any [`Command`](../crates/deadpan-core/src/command.rs)
variant in snake_case), a semantic instruction run with `macro` `apply`, or a
live endpoint operation from [open projects](LIVE_PROJECT.md).

Status meanings:

- **Equivalent**: the headless path reaches the same core/store operation the
  GUI commits, with the same validation.
- **Partial**: the operation is reachable, but the caller must supply what the
  GUI derives (for example a fully resolved event or Source mapping), or part
  of the GUI behavior has no headless form.
- **Gap**: no headless path exists.
- **GUI-only**: navigation, view, audition, preview or personal UI state with
  no project write; headless callers pass explicit context instead.

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
| `ProjectRequest::SoundEdit` | `command` with `set_sound`, `replace_sound`, `delete_sound`, `set_sound_allowance`. R, D. |
| Undo / Redo | `project undo|redo <p> --expected <rev> [--dry-run]`. R, D. |
| Open project (writer held by the app) | Every write above routes to the app's authenticated endpoint (`Execute`, `Prepare`, `Render`, `Generate`) after a writer-lock conflict. |

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
| `y` in Original (copy a source moment, `CaptureOriginal`) | none: no instruction or command writes an Original register | - | Gap |
| `p` / `P`, `:paste`, `:paste-before` (edited content) | `paste` / `replace_selection` instructions, or `splice_slice*` / `replace_slice*` commands | R, D | Equivalent |
| `p` / `P` of an Original moment (`PasteMoment`) | `paste` of an existing Original register, or `splice_source` / `splice_source_at` / `replace_source` with an explicit measured Source node | R, D | Partial (caller derives the Source mapping) |
| `,i`, `:insert` (reuse whole Original) | `command` `insert` with the Original Source node, or `register-source` with `insertion` | R, D | Partial |
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
| `:splice` panel (Copy/Move, replace, seam/interior) | `splice_slice`, `splice_slice_at`, `replace_slice`, `replace_slice_children`, `splice_source*`, `replace_source*`, `move_range` commands | R, D | Partial (panel's picture/audio Before/Proposed audition is GUI-only) |
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
| `:slip ±Nf` panel | `command` `slip_source` | R, D | Partial (stopped-picture preview is GUI-only) |
| `,v`, `:trim`, `:roll` panel | `command` `apply_source_trim`, `trim_source`, `roll_sources` | R, D | Partial (junction pictures, waveform and audition are GUI-only) |

## Holds, pauses and punctuation

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `,h`, `:hold D [video=] [audio=]`, Inspector Insert pause | `insert_pause` instruction (`command` `insert_time` is the lower-level form) | R, D | Equivalent |
| `:hold-duration Nf` | `set_hold_duration` instruction / command | R, D | Equivalent |
| `:hold-silence`, Inspector Use silence | `command` `set_hold_audio` (or `edit_scoped`) | R, D | Equivalent |
| `:room-tone` panel Apply | `command` `set_hold_audio` with a RoomTone range | R, D | Partial (range preparation and audition are GUI-only) |
| `:reverse`, `:ping-pong` | `insert_reverse` instruction | R, D | Equivalent |
| `:tail`, `,t` | `tail` instruction | R, D | Equivalent |
| `:jcut`, `:lcut` | `split_edit` instruction | R, D | Equivalent |
| `,b`, `:bleep` | `bleep` instruction | R, D | Equivalent |

## Camera, framing and attention targets

| GUI | Headless | Checks | Status |
|---|---|---|---|
| Camera panel Enter (`,f`, Inspector Camera) | `command` `set_framing` (or `edit_scoped`) with an explicit pose or envelope | R, D | Partial (interactive draft is GUI-only) |
| `,z`, `,c`, `:zoom`, `:creep` | `set_framing` instruction | R, D | Equivalent |
| `:zoom … target=face:N` | `detect-faces <p> --at <pts>`, then `set_framing` instruction with the saved target | R | Partial (two steps; face save needs a separate `set_target`) |
| `:framing-save a`, `@a` | `macro` `save` of a `set_framing` program, `macro` `run` | R, D | Equivalent |
| `:track`, Camera `T` | `track <p> … [--save <id>]` | R (head at start) | Equivalent |
| Camera `c` (correct region) | `track-correct <p> --target <id> --at <pts> --region <r>` | R (head at start) | Equivalent |
| Camera `n` (new region) | `command` `set_target` | R, D | Equivalent |
| `:track-cancel` | SIGINT to the running `track` process | - | GUI-only |

## Audio and gain

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `+` / `-`, `:gain ±dB`, `:gain +=dB`, `,m`, `:gain-mute`, `:saturate` | `set_audio` instruction (`trim`, `step`, `range_step`, `mute`, `saturation`) | R, D | Equivalent |
| Gain sheet Apply, scoped gain | `command` `set_audio_treatments` / `edit_scoped` | R, D | Partial (Before/Draft audition and waveform are GUI-only) |
| `:edge hard|auto …` | `set_audio_edges` instruction; `set_audio_edge` command | R, D | Equivalent |
| `:audio-lag` | `set_audio_lag` instruction | R, D | Equivalent |
| `:proxies on|off|retry` | none (personal app setting, not project state) | - | GUI-only |

## Placed sounds

| GUI | Headless | Checks | Status |
|---|---|---|---|
| ⌘I Add Sound (`ImportSound`) | `project register-source` with `streams: {"type":"audio_only",...}` | R, D | Equivalent |
| `,s`, `:sound-place` | `command` `set_sound` with a complete qualified event | R, D | Partial (caller builds the event the GUI derives from the catalog sound and cursor) |
| `:sound-at`, `h`/`l` nudges, `:sound-gain`, `:sound-edges`, `:sound-cut` | `command` `set_sound` / `replace_sound` with the changed event | R, D | Partial (no per-parameter shorthand) |
| `:sound-allow`, `:sound-silence` | `command` `set_sound_allowance` | R, D | Equivalent |
| `:sound-delete` | `command` `delete_sound` | R, D | Equivalent |
| `:sting` | `register-source` of a caller-supplied file | R, D | Partial (bundled sting file is app-only) |
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
| `:gag-inspect …` | `apply` dry run of the `gag` instruction returns its trace and edit summary | D | Partial (no outline-only listing) |
| `:gag PRESET`, `:gag-save NAME`, `:gag-presets` | none: presets live in the user's Application Support library, which headless paths never read; pass the recipe explicitly | - | GUI-only by design |

## Analysis and corrections

| GUI | Headless | Checks | Status |
|---|---|---|---|
| Transcribe, detect pauses (background saves) | `transcribe`, `pauses`, `transcript` | - | Equivalent |
| Shot detection | `detect-shots`, `shots` | - | Equivalent |
| `:correct` Enter/`c` (edit text), `x` (remove word/pause), `J` (join), `p` (pause after word), edge nudges, `D` (discard unreadable / drop inapplicable), `u` / `U` | `corrections <p> --json <request> [--dry-run]` with `edit_word`, `remove_word`, `join_words`, `set_word_bounds`, `add_pause_after`, `remove_pause`, `set_pause_bounds`, `drop_inapplicable`, `discard_unreadable`, `undo`, `redo`; `corrections <p>` inspects | R (version and seen target), D | Equivalent (closed project only; see below) |

## AI pauses

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `,a`, `:generate N`, Generate another | `generate-hold <p> --hold <id> [--variants N] [--another]`; live `Generate` | - | Equivalent |
| `:cancel-ai` | SIGINT / live `CancelGeneration` | - | Equivalent |
| `:next-ai`, `:prev-ai`, `:pick-ai N` | `accept-hold --attempt <id>` selects as part of acceptance | - | Partial (no select-only command) |
| `:accept-ai` | `accept-hold <p> --request <id> [--attempt <id>]`; live `AcceptHold` | R (live only) | Equivalent |
| `:discard-ai` | none | - | Gap |
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
| `:recovery` report, Acknowledge | `project validate`, `project originals`, `verify-original` | - | Partial (no acknowledge; the report is app session state) |
| `:backups`, Storage B/J/K/O: back up, choose, restore | `project backup <p>`, `project backups <p> [--verify]`, `project restore <p> <id> [--dry-run]` ([backups](BACKUPS.md)) | R | Partial (restore refuses a project the app has open instead of routing through the live endpoint) |
| `:storage`, Storage… report; P/R project cleanup; C cache cleanup; S, `:portable-copy`, File › Save Portable Copy… | `project storage <p> [--clean [--dry-run]] [--grace-hours N]`, `cache status|clean [--dry-run]`, `project copy-portable <p> <dest>` ([storage](STORAGE.md)) | D | Partial (report and copy run while the app holds the project; `--clean` refuses an open project instead of routing through the live endpoint) |

## Models

| GUI | Headless | Checks | Status |
|---|---|---|---|
| `:models`, Models… panel Install/Resume, from folder/archive, Remove, Discard partial, Cancel | `models list|license|install|import|export|remove [--partial]`, SIGINT | - | Equivalent |

## Counts

Counting each action row once (95 rows): Equivalent 59, Partial 17, Gap 2,
GUI-only 17. Navigation rows whose motions also exist as instructions count as
GUI-only.

## Open gaps and why

- **Original copy (`y` in Original).** No core instruction or command captures
  an Original register; the native `CaptureOriginal` request reads the
  session's measured qualification. Headless callers instead insert Original
  moments directly with `splice_source*`. Adding a store-backed capture is a
  register-bank write that needs its own qualification admission; it was not
  cheap.
- **`:discard-ai`.** Discard is a durable operational store change in the
  app's generation service with no CLI wrapper yet.
- **Storage cleanup of an open project.** The app cleans on its own writer;
  `project storage --clean` refuses an open project rather than routing
  through the live endpoint.
- **Partial rows** mostly need the caller to build what the GUI derives from
  focus (sound events, Original Source mappings, framing poses). The
  interactive previews and auditions of Trim, Slip, Splice, Gain, Room tone and
  Camera are GUI-only by nature; their commits are reachable.
- **Analysis corrections on an open project.** A committing `corrections`
  change needs the project writer. The app does not expose corrections through
  its live endpoint, so while it holds the project the command writes nothing
  and returns `ProjectAlreadyOpen`, telling the caller to use `:correct` in the
  app or close the project. Inspection and `--dry-run` still work.
