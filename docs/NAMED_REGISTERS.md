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
- Registers are saved in the project. Later edits, Undo and reopening retain
  their historical contents. Closing clears runtime state and pending UI
  confirmations; opening another project restores that project's own bank.

There are at most 26 named slots and one default slot. SQLite deduplicates
canonical capture payloads and bounds their total unique size at 64 MiB.
Exceeding the limit refuses the write before changing the bank or cutting time.
Edited values share immutable `Arc` captures, including the default alias.
Registers hold references and authored structures, not decoded pictures or
copied source media. The project service owns serialization and persistence.

Copies create no timeline revision and leave Undo/Redo unchanged. A cut saves
its deletion and both register aliases in one transaction. Undo restores the
deleted time while retaining the cut copy. Consistent SQLite checkpoints include
the bank. Original values retain their exact historical asset, qualification and
half-open ordinal range; Edited values retain their complete validated capture.
Reopening recaptures provenance and creates fresh session identities. Historical
media still requires normal admission before placement or preview.

[Resolved compound transactions](COMPOUND_TRANSACTIONS.md) can capture an
intermediate edited state. Schema 54 retains that exact capture separately from
timeline revisions, so its copy survives Undo and reopening. Native restoration
and historical placement use the capture reader; a retained intermediate state
cannot become a new interactive capture or live edit target.

An Original unavailable in the current revision remains visible in the bank
with an unavailable label. Placement refuses until its exact qualification is
available again; it never substitutes another source.

## Captured intent

Original and Edited copies both save through the ordered project service. The
pending request retains its destination name separately from the next selected
name. Selecting **b** while a capture for **a** finishes cannot redirect that
result. Placement waits for a pending copy to finish saving before capturing its
source, so fast copy-then-paste cannot silently paste the previous value.

A newer write intent supersedes an older UI confirmation, even if the newer
operation is refused. It cannot cancel queued saving or erase a successful
durable write. Versioned bank snapshots advance independently of confirmation:
an older successful write remains in its slot, while only the matching pending
reply may consume the current selection. Failed writes preserve the most recent
durable bank. Older snapshots and replies from a closed session cannot restore
stale contents. A saved cut also retains its independent durable receipt.

Copy replies must still match their full historical provenance. Cuts publish
their contents only after the atomic deletion and register save. A saved cut's
refresh failure retains the copy and reopen guidance. The underlying
[slice commands](EDITED_SLICES.md) and historical media admission remain
authoritative.

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

Macro content and atomic bounded macro execution, semantic dot-repeat and the
rest of DP-06 remain required. Cross-project transfer and the full crash/recovery
matrix are unqualified. Physical layout and native IME qualification remain
separate from deterministic event replay.
