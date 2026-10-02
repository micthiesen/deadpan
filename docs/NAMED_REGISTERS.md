# Named slice registers

Choose `"a` before copying a moment to keep it in register **a**. Choose `"b`
for another moment. `"ap` pastes **a**; `"a:splice` opens its visible placement
preview. The same names hold Original ranges and editable slices from Your edit,
including complete empty groups. The footer identifies the selected register
and its content type. `:registers` opens the live inventory in Keys.

## Selection and lifetime

- Names are **a–z**. Uppercase selects the same slot; it does not append content.
- `""` selects the default copy. `:register a` and `:register "` are command
  equivalents. The `register.select` prefix is [configurable](KEYMAP.md).
- The name applies to the next copy, picture cut, paste or `:splice` attempt,
  including a refused attempt. Navigation does not consume it. Escape or window
  blur cancels the choice. Native text and IME retain their input.
- Put a count after the register name, such as `"a12x`. Counts before the
  register prefix fail explicitly.
- A successful named copy or cut also updates the default copy. Other named
  slots retain their values. Failed capture or save changes neither slot.
- An empty named register never substitutes the default copy. Its paste or
  placement attempt reports the name and leaves history unchanged.
- These are session registers. Later edits and Undo retain their historical
  contents; closing or switching the project clears all slots and pending work.

There are at most 26 named slots and one default slot. Edited values share
immutable `Arc` captures, including the default alias, and retain the existing
bounded capture admission. Registers hold references and authored structures,
not decoded pictures or copied source media. The UI does not serialize captures.

## Captured intent

There is one pending capture across the bank because the service coalesces copy
updates. The pending request retains its destination name separately from the
next selected name. Selecting **b** while a capture for **a** finishes cannot
redirect that result. A newer write intent supersedes an older pending result,
even if the newer operation is refused. It cannot cancel a cut already queued
for saving or erase that cut's independent durable receipt.

Copy replies must still match their full historical provenance. Cuts publish
their contents only after the atomic deletion saves. A saved cut's refresh
failure retains the copy and reopen guidance. Named registers add no authored
command or history entry; the underlying [slice commands](EDITED_SLICES.md)
and historical media admission remain authoritative.

Command entry captures the selected destination for cut and copy. Paste and
placement capture the actual content, register name and absence at entry.
An asynchronous reply that fills a previously empty slot cannot supply content
to that already-open command. Placement refinement and cancellation leave the
register's immutable source unchanged.

If Escape or window blur cancels a named choice while its command is open,
that captured command loses permission to copy, cut, paste or open placement.
Submitting it reports the cancellation and requires new command entry. It
cannot revive the old name or substitute the default copy. Closing/reopening
the project also invalidates that command's captured name. Cancellation replaces
the old next-action instruction in the status line.

## Remaining full-product work

Persistent registers, macro content and atomic bounded macro execution, semantic
dot-repeat and the rest of DP-06 remain required. This session bank does not
provide crash recovery or cross-project transfer. Physical layout and native IME
qualification remain separate from deterministic event replay.
