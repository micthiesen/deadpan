# Visual slice placement

`:splice` opens an unsaved linked Original insertion at the retained Edit
cursor. Copy a half-open Original range with `v`, motion and `y`, then return
to Your edit. Fast `p/P` paste keeps its existing behavior.

The current implementation accepts child seams in ordinary Sequence groups.
An interior cursor stays exact and shows an explanation; `j/k` selects a seam.
It never silently rounds the insertion to a neighboring beat.

## Keyboard and visible state

| Input | Action |
| --- | --- |
| `i`, `o` | Select included In or exclusive Out for local refinement. |
| `d` | Select the destination boundary. |
| `h/l`, Left/Right | Adjust the selected control by a frame; counts work. |
| `j/k` | Next/previous Sequence slot, including distinct zero-length slots. |
| `f` | Inspect the destination picture without changing placement. |
| `b` | Compare Before and Proposed in the captured destination context. |
| Space | Play/pause; resume at the retained heard sample. |
| Shift+Space | Loop the inserted interval with context around both joins. |
| Enter | Commit the exact prepared proposal once. |
| Escape | Cancel and restore the entry cursors and pane. |
| Tab / Shift+Tab | Traverse native controls; Enter activates the focused button. |

The source strip shows decoded In and Out-minus-one pictures. Boundaries are
zero-based, half-open values; displayed pictures use one-based frame labels.
The destination keeps a larger picture, a named group and Sequence slot, an
insertion marker, and a provisional timeline interval. Original and Edit
clocks, linked picture/sound scope, and the unsaved state remain visible.
Pending or unavailable pictures cannot acquire successful display labels.
Text composition and reserved macOS/Kestrel chords retain input ownership.

## Authority and lifetime

The native UI owns only a local range and destination draft. It sends a
session/project/base-revision/draft/change identity to the project service.
The service retains one exact `SourceMomentInsertionRequest`, qualified media,
proposed document and compiled plan. Preparation creates no authored revision
or undo entry. It does not emit an asset-import completion or change selection.

Picture and audio consume the same genuine `Snapshot::proposed` document.
The picture worker validates its private admission against the exact committed
base, checks the captured receipts, and compiles its own plan on its worker
thread. It cannot turn a proposed picture into a committed Camera target.
The independent endpoint worker has one active request, one replaceable pending
pair and one reply. Both use the shared SDR picture semantics; endpoint work
cannot replace the main picture request.

Only the matching ready identity can commit. The service uses the retained
request, so the saved document is exactly the previewed document. A successful
receipt survives a later workspace-refresh failure, and retransmitting that
success cannot insert a second slice. Undo restores the preceding structure.

Local refinements never modify the copied register. Cancellation revokes the
last **issued** identity, including when a newer local refinement was not yet
submitted. Late endpoint/proposal replies cannot restore an abandoned draft.
A changed project session or revision invalidates the destination, stops
audition and revokes pending picture work. Opening a new draft captures fresh
authority. Audition advances the draft cursor without moving saved editor
cursors or beat selection.

## Remaining specification work

This is partial [§9.7](spec/DEADPAN_SPEC.md#97-visual-slice-placement).
Frame-interior insertion, edited-slice selection and atomic move, replacement,
picture-only and audio-only policies, and Repeat/Retime occurrence destinations
remain required. These controls do not establish completion of DP-05 or DP-20.
