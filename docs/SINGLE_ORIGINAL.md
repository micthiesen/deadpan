# One Original, one evolving edit

[Specification 1.1](spec/DEADPAN_SPEC.md) defines the V1 workflow. Choose one local
video or, when the bundled downloader is integrated, one YouTube video. The full
qualified source becomes the initial timeline. The user changes that existing
video through cuts, repeats, Holds, reframing and sound rather than assembling
a project from unrelated picture sources.

The Original remains immutable. Its moments can be reused elsewhere, sound
effects can come from other media, and accepted AI Hold footage can extend the
edit. No additional original video or still-image import belongs in a new V1
workspace. The existing structural backend remains useful and general; legacy
projects keep all their media, edits and history.

## Authoritative profile and baseline

Database schema 17 introduced an optional strict SQLite
single-source profile plus a workflow discriminator. Database 18 stores core 12
and preserves that profile when replaying schema-17 history. Projects predating
the profile migrate without one and remain generic. No JSON sidecar or global catalog becomes
the authority for an Original's identity.

`ProjectStore::create_single_source` requires an empty automatic project.
`SingleSourceState::AwaitingSource` pins that initial revision without claiming
ready media. A live, session-bound `PreparedSourceRegistration` feeds
`initialize_prepared_source` using caller-supplied identities and a label.
The store derives the complete measured source, final basis, root target and
full insertion. Receipt, asset, initial Source beat, presentation basis, history
and Ready profile commit in one transaction.

Ready pins the asset, qualification, historical initial Source node and baseline
revision/history entry. These are operational workflow facts outside authored
undo/redo. The original Source node can be wrapped, copied or deleted by edits;
its historical identity is not reinterpreted as the currently selected beat.

Undo availability, preview undo and actual undo stop at the baseline. Undoing
all editorial changes returns to the full original; it does not undo source
initialization. Deleting every beat does not reset Ready or allow another video.
Ordinary registration rejects a second picture receipt. Ready transitions also
guard generic command ingress against new picture assets. Same-original reuse,
audio-only receipts and dedicated admitted generated-Hold acceptance keep their
existing semantics and guarantees.

Reopen checks profile bounds, initial and baseline revisions, full measured
source/basis/receipt agreement, and chronology including abandoned branches.
The baseline is the sole history root and the active cursor must descend from it.
Tampered or inconsistent state fails validation rather than silently selecting
a replacement original. Original bytes and accepted artifacts retain the
existing immutable ownership and recovery rules.

## Documents library and preparation

`ProjectLibrary` resolves the user's Documents directory through macOS Foundation
and uses its `Deadpan` child. It never falls back to cwd or the selected video's
directory. Tests inject a Documents path instead of touching user projects.

The service creates packages with bounded sanitized source-derived names,
retrying numbered suffixes only after the store's exclusive package-create
collision result. It does not retry partial initialization or permission errors
as if they were naming collisions. The original display label remains separate
from the package filename. Existing projects elsewhere can still be opened;
they are not moved automatically. Generic headless creation retains explicit
developer paths.

New selects the source before allocating a project, so canceling the picker
creates nothing. `CreateFromSource` allocates an Awaiting Source package, prepares
retention and measured media off the writer, then commits initialization.
`InitializeSource` retries an incomplete package with captured session and
revision. Failures preserve useful retained bytes and the explicit incomplete
state. No successful receipt, progress string or visible frame alone is Ready.
Switch/close/cancel ordering rejects stale preparation and retains sole worker
ownership. Initialization completion explicitly selects the resulting Source
and opens Your edit.

## Native interface and keyboard

The [single-original imagegen targets](design/README.md) guide the native layout:
one Original rail, large picture, compact editable beat cards, useful conditional
inspector, separate audio collection and persistent status/key hints. Original
and Your edit are visible labels for Source and Sequence contexts. Their clocks
and durations remain separate. Duration differences are derived from measured
original placement and the current edit, never guessed from a container label.

Common actions teach their keys beside the control. Pending operators show the
exact prefix and valid next input without timing out. Pane focus is distinct
from selected media or beats. Text, IME and native composition keep their own
input, including after a pointer event changes focus. Inspector parameter entry
uses the same command field and typed transactions as keyboard commands.

Whole-original reuse remains a real supported command. Arbitrary moment copying,
range paste, nested navigation and the rest of the specified grammar require
their corresponding command implementation; a concept board's `y` label cannot
stand in for that work. Generic/legacy projects have an explicit compatibility
view that preserves their broader source catalog and existing insertion path.

## Sound effects and remaining scope

Audio-only registration is distinct from placing a sound event. The Original
must stay selected as the picture source when a sound import completes. Import
must not add blank picture, lengthen the sequence, or replace original speech.
The generic backend's audio-only sequential Source beat remains available to
developer and compatibility workflows; it is not an overlay implementation.

The current native decoder's admitted sound formats remain PCM16 WAV and the
guarded AAC/MP4 path. A filter or newly convenient track selector does not qualify
MP3, FLAC or another container grammar. Exact source clocks, priming evidence,
verified bytes and bounded decoding remain required.

Native sound import chooses the first actual admitted audio track, including
MP4 with video at stream 0 and audio at stream 1. `AudioSession::open_first_input`
and `AudioDecoder::open_first` resolve it through the same bounded complete
container admission pass as explicit selection, sharing opening byte/deadline
budgets. They do not probe guessed streams or weaken allocation guards. Explicit
stream selection remains available through the backend.

The full V1 still requires sound-event authoring/mixing/placement, local YouTube
acquisition, arbitrary ranges and splices, analysis, app generation/audition,
playback, export and release qualification. These remain explicit in
[Requirements](REQUIREMENTS.md). A focused interface does not remove them or
justify exposing a control that pretends they work.
