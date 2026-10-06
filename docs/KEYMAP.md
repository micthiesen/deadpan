# Editor keymaps

Deadpan reads an optional `Deadpan/keymap.json` from the macOS user Application
Support directory before opening its native window. On an ordinary installation
this is `~/Library/Application Support/Deadpan/keymap.json`; the app resolves it
through `NSFileManager`, independently of the project and current directory.
It does not create a file or directory. Restart Deadpan after editing the file.

A missing file uses the shipped keys. A read, schema or binding error rejects the
entire candidate. All shipped keys stay active, and the header's **Keymap error**
control opens the persistent diagnostic in Keys. Ordinary edit messages cannot
erase that diagnostic. Headless commands, worker processes, UI replay and the
native lifecycle smoke test do not read personal settings.

## Configuration

```json
{
  "version": 1,
  "key_mode": "logical",
  "bindings": [
    { "action": "frame.next", "keys": [["a", "h"], ["ArrowRight"]] },
    { "action": "hold", "keys": [["e", "b"]] },
    { "action": "command", "keys": [["e", "c"], [":"]] }
  ]
}
```

Each entry replaces every alias for one action; omitted actions retain their
shipped paths. The example makes `ah` move forward and `eb` insert a pause.
`3ah` moves three frames and `3eb` requests three half-seconds. Counts, authored
commands and held-key policies belong to the action, not to configuration.
Overrides are applied together, so two actions can exchange their paths.

Paths contain 1 to 16 key tokens, with 1 to 8 aliases per action. Lowercase
letters are unshifted; uppercase letters require Shift. Named keys include
`ArrowLeft`, `Space`, `Home` and `F2`; `Shift+Space` is explicit. Punctuation
can use its symbol. Digits belong to the count grammar and cannot be path steps.
Control, Command and Option chords remain reserved. Escape, Tab and Shift+Tab
keep their native editor roles. Actions cannot be unbound in this format.

`mark.set`, `mark.jump` and `register.select` configure the prefix before a single
letter argument. Register names are `a–z`; uppercase selects the same lowercase
slot, and a double quote selects the unnamed register. Marks keep distinct
`a–z` and `A–Z` names. These families expand before ambiguity and resource checks,
including the final argument's path length. No prefix timer executes a partial
command. The double quote token (`"\""` in JSON) and `Shift+Quote` are equivalent.
The 16-key bound also applies to each composed operator-plus-motion path.

| Action IDs | Meaning |
| --- | --- |
| `frame.previous`, `frame.next` | Frame motion; sound nudges in Placed sounds |
| `beat.previous`, `beat.next` | Beat or catalog/event selection |
| `word.next`, `word.previous`, `word.end` | `w` / `b` / `e`: recognized word starts and the next word end, in Original and Your edit; counts move further and compose after `y`, `d`, `r` |
| `sentence.next`, `sentence.previous` | `W` / `B`: recognized sentence starts |
| `object.inner_word`, `object.around_word`, `object.inner_sentence`, `object.around_sentence` | `iw` / `aw` / `is` / `as`: Visual word or sentence at the Edit cursor, with up to 80 ms pause handles for `a`; also compose after `y`, `d`, `r` |
| `pause.next`, `pause.previous` | `]p` / `[p`: start of the next / previous detected pause, in Original and Your edit; counts move further and compose after `y`, `d`, `r` |
| `play.next`, `play.previous` | `]r` / `[r`: step the nearest open Repeat through All plays, then play 1..N (counts step further); on a selected Repeat beat, `]r` opens play 1 and `[r` the last play, and a count `N` opens play N or the Nth play from the end. Navigation only: operators never compose with it and Macros do not record it |
| `object.inner_pause`, `object.around_pause` | `ip` / `ap`: Visual pause at the Edit cursor, with up to 80 ms of the adjoining speech for `a`; also compose after `y`, `d`, `r` |
| `shot.next`, `shot.previous` | `]s` / `[s`: start of the next / previous detected shot occurrence, in Original and Your edit; counts move further and compose after `y`, `d`, `r` |
| `ai.generate` | `,a`: generate AI pictures for the selected pause (Hold) in Your edit, in the background; Normal Edit only, no count. `:generate [N]`, `:cancel-ai`, `:next-ai`, `:prev-ai`, `:pick-ai N`, `:preview-ai`, `:audition-ai`, `:accept-ai` and `:discard-ai` complete the workflow |
| `ai.compare`, `ai.next` | `,x`: compare the selected pause's committed picture (Before) with the chosen AI variant at the same frame and heard sample, previewing it first when needed; `,n`: show the next variant the same way. Normal Edit only, no count; `:compare-ai [before\|N]` chooses directly |
| `repeat.escalating` | `,e`: wrap the selected beat or Visual range in three plays, each 3 dB louder and 0.08 closer, as one Undo |
| `gain.mute` | `,m`: mute the Visual range inside the selected beat as a mute range, or toggle the whole beat's mute (`:gain-mute`); Your edit only, no count |
| `cutaway.pick` | `,r`: open `:cutaway register=` on a register holding a copied Original moment, listing the others; Your edit only, no count |
| `bleep` | `,b`: bleep the Visual range (`viw` for a word): its pictures keep playing over a 1 kHz tone at -10 dB as one Undo; `:bleep 880Hz level=-6dB` chooses the tone; Your edit only, no count |
| `tail` | `,t`: open `:tail` with the selected pause's length (or 1s) and `effect=reverb` ready to change; Enter gives that pause a hanging tail or inserts a tail pause at the cursor; Your edit only, no count |
| `object.inner_shot`, `object.around_shot` | `iS` / `aS`: Visual shot occurrence at the Edit cursor (hard cuts carry no transition handles, so they match); also compose after `y`, `d`, `r` |
| `first`, `last` | Start and end |
| `undo` | Undo; native undo/redo alternatives remain fixed |
| `playback`, `audition` | Play/pause and selection loop |
| `group.enter`, `group.leave` | Group navigation; exact event position in Placed sounds |
| `group.create`, `group.ungroup` | Name a selected beat/range with `,g`; neutral Ungroup has a command alias and no shipped key path |
| `structure.explode`, `structure.duplicate` | `:explode` turns the selected Repeat into an ordinary group of its plays; `:duplicate` copies the Visual range, or else the selected beat, after itself. Command aliases only, no shipped key path or count; both are recordable and dot-repeatable |
| `visual`, `copy` | Select time; immediate Original or Visual copy |
| `object.inner_group`, `object.around_group` | `ig` / `ag`: Visual group contents / whole group; also compose after `y`, `d`, `r` |
| `copy.beat`, `yank.operator`, `cut.operator` | Whole-beat copy and typed motion prefixes in Normal Edit |
| `paste.after`, `paste.before` | Paste, or replace a captured Time range or group Object |
| `split`, `cut.frames`, `cut.beat`, `cut.range` | Structural edits; range cut is Visual, beat cut is Normal |
| `edit.repeat-last` | Repeat a picture cut, Repeat wrap/count change, Group, Ungroup, Explode or Duplicate at the current eligible target; default `.`, no count or held activation |
| `repeat`, `hold` | Total plays and inserted pause |
| `insert`, `sound.place` | Reuse the Original and place a catalog sound |
| `gain.up`, `gain.down` | Gain steps |
| `camera`, `punch_in`, `creep`, `trim` | Enter a draft or apply the framing action |
| `mark.set`, `mark.jump` | Letter-mark prefix families |
| `register.select` | Select the register for the next copy, picture cut or paste |
| `command`, `search`, `help` | Native command/search entry and Keys; with a transcript, `search` focuses Find words |
| `search.next`, `search.previous` | `n` / `N`: next / previous transcript match after the cursor, in Original or Your edit |
| `pane.next`, `pane.previous`, `escape` | Fixed paths; included in the semantic catalog |

## Logical and physical keys

One `key_mode` applies to the complete map. `logical` follows the delivered egui
key identity. The pinned egui-winit 0.36 adapter falls back to a physical identity
when a character or dead key has no named egui key: AZERTY `&` arrives as Num1,
QWERTZ `"` as Shift+Num2 and QWERTZ `ö` as Semicolon. In logical mode the
press's immediate `Text` companion therefore decides. `@`, `"`, `'`, `_` and `>`
map to their existing strokes whatever Shift the layout needed. Text that names
the delivered key keeps ordinary modifier handling, so AZERTY Shift+3 is a count.
A letter of a non-Latin script (Cyrillic, Greek, Hebrew, Arabic and so on)
has no egui key, so egui-winit delivers its physical position, and that
position stands in for the Latin letter, as egui's own fallback intends: Russian
`р` on the H key is `h`, `пп` is `gg`, Shift+`п` is `G`, and Greek `η` or Hebrew
`י` on H is `h` too. Typed punctuation on those layouts keeps its logical
meaning (Russian `.` and `,` are the period and comma wherever they sit).
Any other companion means a character no binding can name, and the press is
inert: AZERTY `&é(§çà`, QWERTZ `üöäß#`, Turkish `ı`, Russian Shift+3 `№`, US
`#` and `<` and US Option+`[` (`“`) neither count nor act. Latin-script letters
stay inert because their layouts type every ASCII letter elsewhere. QWERTZ `'`
(Shift+`#`) is a mark jump, not `"`.
A press without companion text keeps the delivered identity. If macOS reports
a dead key such as French `^` as a key press, it can still reach the binding at
its position; dead keys are not replayed.
Brackets join the logical symbols, so QWERTZ Option+6 and AZERTY
Shift+Option+`)` produce `]`. A personal logical map may still contain
`Shift+[` or `Shift+]`, which compiled before brackets became logical symbols;
the file loads, but the Keys sheet's keymap status shows a **Keymap warning**
because those strokes can never match (Shift+`[` types `{`): bind `"{"` or
`"["` instead. Other shifted punctuation (`Shift+;`, `Shift+=`, `Shift+\`,
``Shift+` ``) loads with a warning that it matches only on a layout whose Shift
types that same symbol. The other logical symbols with Shift remain errors.

Kestrel reservations are checked on the physical position first, so some layout
characters stay unavailable while Kestrel runs: QWERTZ `[` (Option+5) and `@`
(Option+L), and AZERTY `[` (Shift+Option+5) and `{` (Option+5). AZERTY `@` is
unshifted and works. When Deadpan receives such a press (Kestrel is not running
or did not intercept it), the status line says that Kestrel reserves the chord
and teaches the alternative instead of ignoring it silently: for `[r`, `:scope
play N` or `:scope all`; `[p` and `[s` have no command, so bind
`pause.previous`/`shot.previous` elsewhere or go back with `gg` and a counted
`]p`/`]s`; for `@`, `:macro a`. The press itself still does nothing.

The fixed mode routers for Camera, Trim, Slip and Place slice read the same
companion text before matching: AZERTY Shift+3 is the digit 3 in Camera's
rectangle fields and Place slice counts, unshifted `&é"(` name no key there,
and a non-Latin letter acts at its position (Russian `л` on L nudges Trim).
Gain and room tone route only Enter, Space and Escape, which carry no layout
character, so they need no translation. Corrections, the YouTube URL step and
the Marks modal still match the delivered key.

Egui uses `Quote` for both apostrophe and double quote. In logical mode the
companion text decides; without it, and in physical mode, Quote preserves Shift:
plain apostrophe opens mark jump and Shift+Quote opens register selection. Option+Quote remains with native input. This fallback does not prove
quote behavior on every physical layout.

`physical` requires the native physical key field and never substitutes a
logical key when it is absent. It names keyboard positions. Physical defaults
use `Shift+Semicolon`, `Shift+Slash` and `Shift+Equals` for command, help and gain
up. Use those positional forms in overrides. A map cannot mix logical and
physical matchers, which could overlap on another layout. Physical `Plus` names
the keypad Add key and is displayed as `NumpadAdd`. Native text editing,
composition and menu shortcuts keep their own input semantics.

## Shared grammar

Normal and timeline Visual editor paths compile from
[`editor_map.rs`](../crates/deadpan-app/src/navigation/editor_map.rs) through the
bounded [`binding_trie.rs`](../crates/deadpan-app/src/navigation/binding_trie.rs).
Each terminal carries a typed action, count policy, short explanation and held-key
policy. Proper prefixes carry separate notifications and semantic capture roles.
Normal Edit operator terminals resolve typed selectors through the shared
semantic planner and native project service.

## Action registry

Every user action is described once in the
[action registry](../crates/deadpan-app/src/navigation/registry/entries.rs):
its id, name, keys, command verbs and usage, the contexts where it applies,
macro and dot behaviour, its headless equivalent ([parity](PARITY.md)) and its
help text. Configurable keys are named by action id, and help text names keys
by placeholder (`{cut.operator!}{object.inner_pause!}`), so a personal keymap
changes the Keys sheet, footer teaching and examples together. The command
parser admits exactly the registry's verbs and completion lists their usages.
[COMMANDS.md](COMMANDS.md) is generated from it by a test that fails when it is
stale. `DEADPAN_UPDATE_COMMANDS_MD=1 cargo test -p deadpan-app --bin deadpan-app
commands_reference` rewrites it and then fails so the diff is reviewed; under
`CI` it refuses.

Fixed mode keys are declared there as exact chords: Camera, Trim, Slip, Place
slice, room tone, the Gain draft, transcript corrections, the Marks, Jobs and
Storage panels and the Keys sheet. A test drives every egui key through 11
modifier combinations (including Control+Option, Control+⌘, Option+⌘ and each
⌘ chord with and without macOS's separate Command flag) and every router state
flag through each mode's real router. It requires that the router acts on
exactly the declared chords, that each chord has one owner, and that the
owner is the action the router actually takes. The Marks, Jobs and Storage
panels and the Keys sheet read their keys from those declarations through
typed tables that a test proves cover exactly them; mode footers and
Apply/Cancel buttons take their key labels from them, and a test checks every
label the interface paints. Mode keys are not configurable: the specification
fixes Camera's and Trim's keys, and the panels follow the same rule. Native
shortcuts (menus, ⌘ chords, Models and YouTube panel controls) are described
in the registry but not router-verified.

`routing_matches_the_pre_registry_baseline` compares a checked-in snapshot.
Its first part was taken before the registry existed: every shipped trie path
in logical and physical modes, labels, prefix teaching, count resolutions, the
former verbs with their usages, about 560 parse results, completions, and the
pure Camera, Trim, Slip, Place slice, room-tone, Gain and corrections router
decisions. Later sections were appended without changing those lines: more
modifier combinations, Camera under text and composition, Camera's
`dispatch_key` (field, activation and global-chord answers), `mode_key`'s
companion-text translation for 16 typed characters, held-key state through
`Bindings::route_event`, and the registry-driven panels. It does not pin
digit-count accumulation inside the Trim, Place slice or Camera drafts beyond
the router decisions, native menu shortcuts, or the Models and YouTube panels.
An intended change regenerates it with `DEADPAN_UPDATE_ROUTING_BASELINE=1`
(the test then fails until rerun, and refuses under `CI`) and a reviewed diff;
added verbs leave it unchanged. Two changes have been made this way: corrections
⌘⇧Z (below) and the appended sections.

The Keys sheet (`?`) lists every action by section with its live keys and
verbs. Each row is one accessible label: name, help, then notes marking in
lavender what works in the current context, its contexts elsewhere,
macro/dot behaviour and headless parity; hovering shows the headless form.
`/` opens the "Search actions" field: every word must match. A term equal to a
key path (`dd`, `,h`, `G`, case-sensitive) ranks first, then a verb, a name,
and any text. A query of only terms under three characters is a key lookup:
it matches keys and exact verbs, never prose or verb prefixes, so `dd` lists
only the whole-beat cut and `s` only Split; with a longer word present, short
words match prose too. `:` alone lists every verb and `:ho` matches verb
prefixes. The result
count is a live region announced as it changes, and the key line names the
field's own keys while it has focus. Enter keeps the filter and returns to
scrolling; Escape in the field leaves it, including when `/`, text and Escape
arrive in one batch (what follows the Escape is Help's again), and Escape again
closes the sheet. A held `/` never echoes into the field. A layout that shifts
`/` reaches search by its typed character. While the field has focus it owns
every key and IME event; no widget drawn before it sees them.

Router facts the registry made explicit: Slip's `h`/`l`/arrows and Place
slice's Shift-Space use egui's `shift_only`, which also admits Control; this is
preserved. The corrections sheet compared ⌘⇧Z with egui's `COMMAND | SHIFT`
exactly, so a macOS press, which also sets the separate Command flag, did not
redo there. It now uses the app's shared `navigation::native_command` test, as
the editor's ⌘ shortcuts do, so ⌘Z undoes and ⌘⇧Z and ⌘R redo with either flag
form. The Storage panel formerly used egui's logical key match, which ignores
Option and Shift, so Kestrel's Option+P and Option+S acted as Preview and Save
copy and Shift+R confirmed a removal. Storage keys now act only unmodified, are
read by their typed character like Jobs (so non-Latin layouts reach them at
their positions), and a held key never repeats an action.

## Input ownership

The host gives native controls, text and IME priority before an editor command
can run. Platform menu shortcuts and logical-symbol handling remain explicit
in the outer router. A physical Kestrel reservation is checked before logical
normalization, even when the delivered logical key differs. Camera, Trim,
Gain, room-tone and Place slice keep their own mode routers. Camera's keys,
including the target keys `n` (new rectangle), `t` (follow), `c` (correct
here) and Shift+`T` (track), are fixed in its router and not part of the
configurable trie; the registry declares them and a test proves the router
matches; `:track` and `:track-cancel` are ordinary commands.

Native Command shortcuts are fixed: `⌘N` (choose video), `⌘⇧N` (start from a
YouTube URL, also `:youtube`), `⌘O`, `⌘I`, `⌘E`, `⌘Z` and `⌘⇧Z`. The YouTube
URL step has its own router ahead of the editor: plain Enter runs its current
step unless a native button is focused, plain Escape cancels or leaves it, and
held keys, modifiers and composition never act. Neither is configurable.

Mark names resolve before ordinary editor actions. `mx`, `'d` and uppercase
names remain marks. Escape, Tab and native menu actions retain their defined
interrupt behavior. A valid continuation takes priority over a root action;
otherwise transport and group navigation interrupt ordinary pending paths.
A pending mark requires a letter. A pending register accepts a letter or double
quote. Invalid suffixes clear their prefix without executing the suffix as a new
root command. Register selection never captures a mark position.

The app captures Trim's exact target at the first pending ancestor of a Trim
path, or immediately before dispatch for a direct key. Mark capture starts
when the complete letter-family prefix is entered. Captured absence is a real
result: a delayed service reply cannot supply a target missing on entry.
Choosing another branch discards that capture. Pending paths retain their
entry Normal/Visual mode. Compilation and help never mutate a project.

`"a` selects project register `a` for the next yank, picture cut, paste or
`:splice`; `""` selects the unnamed register. `:register a` and `:register "`
are command aliases, and `:registers` opens the register inventory in Keys.
Selection is one-shot and Escape cancels it. A successful named copy or cut also
updates the unnamed copy. Register selection rejects all preceding counts;
put a supported count after the name, such as `"a12x`. The project register bank
persists across app launches, including typed Macro contents.
See [named registers](NAMED_REGISTERS.md) for durability and validation rules.

`q` plus a letter starts recording, and `q` while recording saves to that named
Macro. `@` plus a letter runs it; a positive preceding count repeats the call
in one transaction. The configured `macro.record` prefix itself stops recording.
Recordings support frame/beat motions, Visual selections, typed copy/cut selectors,
register pastes/replacements and named calls. Prefix and command
entry capture the exact project, bank and cursor, including absence. Logical
`@` requires the key's immediate `Text("@")` companion; physical mode binds
Shift+2. See [semantic macros](SEMANTIC_MACROS.md) for limits and typed errors.

Entering Command or Search gives the new field only the ordered input suffix
after its opener. The opener's immediate printable companion text is consumed;
later text, paste, composition and key events remain native input. A held opener
does not echo into the field before release, including when Shift changes its
logical identity or the field closes first. Enter/Escape gives the closing field
only its preceding input; the remaining events resume in order on the next outer
frame. Layout retries cannot replay that suffix. Existing whole-batch IME priority
remains conservative:
any composition event prevents editor submit/cancel for that batch.

While the command line holds only a partial verb, the footer lists the
matching commands with their usage (`:cap` shows `:caption TEXT [at=top|center]
[delay=4f]`), from the [action registry](#action-registry) that also admits
verbs in [`command.rs`](../crates/deadpan-app/src/navigation/command.rs). A unit
test checks that every listed verb parses, and a drift test scans the parser's
verb literals so a parsed verb missing from the registry fails. Commands without
a key path are therefore discoverable by typing their first letters, besides the
Keys sheet and its search. `:close`
closes the open project with the same readiness as File › Close Project (it refuses,
naming them, while unsaved previews such as Camera, Gain or Trim drafts, a Render
decision, a recording, a save or an open panel are pending, and never discards a
draft) and `:sound-channels mono|stereo|none` sets how the next
sound without a declared speaker layout is heard, so neither needs the pointer.

## Counts, repetition and teaching

Count policies preserve the existing distinctions between absent count, zero,
one, a larger count and overflow. Motions, Repeat, frame cuts, whole-beat cuts,
Holds and gain use their declared policies. A positive Repeat count still means
total plays. Yank/cut distance counts may appear before the operator or after its
complete prefix, immediately before the motion. Supplying both rejects rather
than multiplying. `yy`/`dd` accept only no count or one, and `gg`/`G` operator
motions reject counts. Other operators retain their own count policies.

After an explicit frame/beat motion, held events retain that resolved action
until release, intervening input or context loss. The first execution uses its
count; later repeats move one unit. Thus holding the final `h` in a custom `ah`
forward motion keeps moving forward even when root `h` means backward. Held
input cannot enter or consume a new pending path. Holding `h` while pressing
comma cannot insert a Hold; release and a fresh press are required.

The Normal Your edit footer also teaches the operator prefixes (`d / y` with
any motion or object), Macro record/run, punch-in/creep and, on a selected
Repeat, `]r [r`. Within a footer tier earlier hints win the two-row budget, so
the operator and Macro pairs precede Camera, Trim, gain, register and group
hints; the Keys sheet keeps every path.

Prefix hints derive their next keys and explanations from the declarations.
Counted comma teaching exposes its valid Hold continuation. A count that leaves
no valid completion, such as `0d` or `0r`, displays the actual refusal instead of
advertising an edit. The root zero-count hint explains its unusual motion and
zero-time semantics. Native-control cut protection probes the typed action
without consuming pending input, preserving mark-name authority.

In Normal Edit, `y` and `d` are pending operators. `yy` copies the selected beat
with owned attachments; `dd` cuts it. Their frame, beat and group-boundary
continuations compose with the configured motion paths. `copy.beat` and
`cut.beat` remain independently configurable terminals; `yank.operator` and
`cut.operator` configure the motion prefixes. The same trie rejects conflicts
between generated continuations and explicit paths. Visual `y`/`d` and Original
`y` retain immediate behavior. Sounds retain their own copy refusal and `dd`
removal. A pending domain change refuses the next continuation, so it cannot
become a copy in a newly active pane. Visible copy teaching chooses the complete
`yy` or `y` path for the actual domain and selection.

## Compiler and audit

The compiler rejects empty paths, duplicate terminals, and any terminal that is
also a prefix, in either declaration order. Prefix annotations must identify an
existing proper branch, are unique and never create a hidden command. Errors
identify both conflicting labels and complete typed key paths.

The file reader opens nonblocking, then checks that the descriptor is a regular
file. A 256 KiB limit applies both to metadata and actual bytes read. This is a
size bound, not a filesystem deadline or coherent-snapshot guarantee. JSON
keeps its recursion bound, rejects unknown/duplicate fields and bounds action
and key tokens before constructing diagnostics.

Compiler limits are 512 terminal definitions, 128 prefix annotations, 16 keys per path,
4,096 distinct nodes including the root, and 256 UTF-8 bytes per label. Resource
preflight runs before internal allocation or diagnostic key/label cloning.
Shared prefixes count once. Flat indexed nodes avoid recursive ownership and
traversal. These bounds cover the compiler's container; callers still own input
allocation and arbitrary payload limits.

The Kestrel audit enumerates every structural branch in both compiled maps,
including branches without annotations. It tests each with absent, positive,
zero and overflowing counts, alongside text, composition, Visual selection and
the existing modal routers. The live registry digest remains a separate check.
See [shortcut compatibility](KEYBINDING_COMPATIBILITY.md).
The [configuration qualification](qualification/configurable-bindings-2026-10-01.md)
retains startup, custom-map and native-input checks and their limits. The earlier
[compiler qualification](qualification/declarative-bindings-2026-10-01.md)
retains the original held-key regression.

## Remaining work

Current configuration covers Normal and timeline Visual paths and their teaching.
Mode keys are declared and verified in the registry but fixed. Command
arguments still use per-verb parsers rather than a schema-driven typed-unit
grammar, completion lists verbs but not parameters or the default selector,
and the searchable Keys sheet describes actions without executing them.
The `layouts` replay drives German QWERTZ and French AZERTY presses, as
egui-winit 0.36 delivers them on macOS, through the production router, and
drives Russian ЙЦУКЕН letters at their physical positions, and composes
Japanese text in Command (`:caption`), the transcript word field and the YouTube
URL field. egui applies a field's focus filter before the app reads a native
batch, so a filter that followed composition could not keep focus when a
Preedit and its Escape arrive together. The word and URL fields therefore keep
focus through Escape like Command, installed in the frame focus is acquired;
their sheet routers handle a plain Escape outside composition and leave the
field explicitly, and the replay sends Preedit and Escape in one batch.
egui-winit 0.36 emits no IME Enabled/Disabled events. These replays inject the
events egui-winit would deliver: physical keyboard delivery on real layouts
(including the macOS input-source switcher and dead keys) and a real OS input
method remain unverified. Settings are file-based and require a restart; a
native settings editor and live map replacement are not implemented. Semantic
[dot-repeat supports picture cuts](SEMANTIC_REPEAT.md); [macros](SEMANTIC_MACROS.md)
support frame/beat/group motions, Visual selections, copies, cuts, pastes and
named calls. Remaining edit kinds and selectors
remain DP-06 work. No requirement
or product gate is complete on the basis of this increment.
