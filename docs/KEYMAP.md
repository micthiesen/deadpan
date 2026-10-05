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
| `object.inner_pause`, `object.around_pause` | `ip` / `ap`: Visual pause at the Edit cursor, with up to 80 ms of the adjoining speech for `a`; also compose after `y`, `d`, `r` |
| `shot.next`, `shot.previous` | `]s` / `[s`: start of the next / previous detected shot occurrence, in Original and Your edit; counts move further and compose after `y`, `d`, `r` |
| `ai.generate` | `,a`: generate AI pictures for the selected pause (Hold) in Your edit, in the background; Normal Edit only, no count. `:generate`, `:cancel-ai`, `:preview-ai`, `:accept-ai` and `:discard-ai` complete the workflow |
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
| `visual`, `copy` | Select time; immediate Original or Visual copy |
| `object.inner_group`, `object.around_group` | `ig` / `ag`: Visual group contents / whole group; also compose after `y`, `d`, `r` |
| `copy.beat`, `yank.operator`, `cut.operator` | Whole-beat copy and typed motion prefixes in Normal Edit |
| `paste.after`, `paste.before` | Paste, or replace a captured Time range or group Object |
| `split`, `cut.frames`, `cut.beat`, `cut.range` | Structural edits; range cut is Visual, beat cut is Normal |
| `edit.repeat-last` | Repeat a picture cut, Repeat wrap/count change, Group or Ungroup at the current eligible target; default `.`, no count or held activation |
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
key identity. The pinned egui-winit adapter falls back to a physical identity
when a character or dead key has no named egui key. This is a known limit; it
does not establish strict logical-key behavior for every layout.

Egui uses `Quote` for both apostrophe and double quote, so Quote preserves Shift
in both key modes: plain apostrophe opens mark jump and Shift+Quote opens register
selection. Option+Quote remains with native input. This fallback does not prove
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

## Input ownership

The host gives native controls, text and IME priority before an editor command
can run. Platform menu shortcuts and logical-symbol handling remain explicit
in the outer router. A physical Kestrel reservation is checked before logical
normalization, even when the delivered logical key differs. Camera, Trim,
Gain, room-tone and Place slice keep their own mode routers. Camera's keys,
including the target keys `n` (new rectangle), `t` (follow), `c` (correct
here) and Shift+`T` (track), are fixed in its router and not part of the
configurable trie; `:track` and `:track-cancel` are ordinary commands.

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
The remaining mode routers, strict logical provenance and physical layout/IME
qualification remain open. Settings are file-based and require a restart; a
native settings editor and live map replacement are not implemented. Semantic
[dot-repeat supports picture cuts](SEMANTIC_REPEAT.md); [macros](SEMANTIC_MACROS.md)
support frame/beat/group motions, Visual selections, copies, cuts, pastes and
named calls. Remaining edit kinds and selectors
remain DP-06 work. No requirement
or product gate is complete on the basis of this increment.
