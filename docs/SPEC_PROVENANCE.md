# Design package provenance

The complete Deadpan design package, version 1.0 dated 20 September 2026, is
preserved in `docs/spec/archive/1.0/`. All five files are byte-for-byte copies from
`Deadpan_Design_Package.zip`. The original package README, handoff, keyboard
reference, Markdown specification, and PDF reading edition retain their names
and relative links.

The current [Markdown specification](spec/DEADPAN_SPEC.md), version 1.1 dated
23 September 2026, is normative. The product owner requested a single-original
V1 workflow: choose one video, automatically start with its entire unchanged
timeline, then reshape it through reversible edits. Same-original reuse, external
audio effects and accepted AI extensions remain in scope; extra picture sources
do not. New native projects use the system Documents/Deadpan library. The revised
specification, handoff, keyboard reference and interface targets integrate that
policy rather than maintaining a competing addendum.

On 26 September 2026 the product owner made holistic offscreen UI replay the
normal agent feedback loop and required compatibility with Kestrel's global
shortcuts. The current keyboard policy reserves those globals; whole-Original
reuse uses `,i` in place of the conflicting native `⌘Return` accelerator.
[UI feedback](UI_FEEDBACK.md) defines the executable evidence and its limits.

On 27 September 2026 the sound-placement interaction target was integrated into
Section 7.5 and the keyboard reference: `,s` places catalog audio at the retained
Edit cursor, with a separate placed-event selection, exact frame nudges and fine
sample entry. This elaborates the authorized single-Original audio workflow and
visible Vim-style controls. It does not reduce the required sound ownership,
silence, effects or export scope. Current implementation remains a root-event
subset tracked separately in [sound events](SOUND_EVENTS.md).

The same interaction grammar now names `:sound-allow` and `:sound-silence`.
They expose Section 10.4's explicit exception for one sound in one concrete
silent pause, with captured Edit-frame scope and visible inspector feedback.
This adds discoverable controls without changing default silence or authorizing
permission for other voices, other occurrences, or absent routed samples.

On 28 September 2026 the product owner explicitly approved the Section 22.3
muxing rule: “Explicitly validated stream start and sync; edit lists permitted
for encoder delay, padding, and frame reordering.” The approved text was applied
on 29 September after a requested pause. Full emitted-file verification and
encoded synchronization tolerances remain required. The
[FFmpeg measurements](qualification/encoder-timing-2026-09-28.md) and
[independent AVFoundation comparison](qualification/native-audio-2026-09-28.md)
retain both the aligned reference and failed no-edit-list files. This policy
change does not establish a working product export path or alter those results.

The original five files were hash-verified before archiving. Their names and
relative links are retained there. The root `spec/DEADPAN_SPEC.pdf` also remains
an unchanged historical 1.0 copy for old links; it is not an updated reading
edition. Preserve the archive unchanged. Record later deliberate product revisions
here. Implementation status remains separate in [REQUIREMENTS.md](REQUIREMENTS.md).

On 2026-10-05 the owner decided that Deadpan is a personal application with
no Apple developer account: it is not distributed, needs no Developer ID
signing, notarization or stapling, and is signed ad hoc or, when a stable
signature is needed, with the owner's local signing identity from dotfiles
(the same mechanism as Kestrel). The distribution row, §15.2 updater testing,
§26.6 clean-machine wording, §27.3 packaging and DP-22 were revised to match.
Licensing notices and the SBOM remain, since the bundle still ships
third-party code.

On 2026-10-05 the owner also limited required verification to what can be
exercised on this machine (§29.1 Verification scope). Checks needing a clean or
second Mac, other hardware tiers, physical keyboards/IMEs, VoiceOver speech,
listening or human judgement, rights-cleared real-person footage, physical
power/display tests or unavailable accounts are recorded as To verify (owner)
and do not block completion; §13.4, §25.1, §26.2 fixture text and §26.6 were
revised to match. Implementation scope is unchanged.

Source archive SHA-256: `7c44444cdbabaa7196910fb7affefc03f0c920169f2ffa2a710ea6bccd236046`.

On 2026-10-18 the implementation adopted the existing handoff's decision 4
recommendation for ranged zoom/creep macros: retain center/scale/curve intent,
resolve the new invocation range, allow retained-center and `target=center`,
and refuse ranged sampled-target recording explicitly. Section 7.4 now records
that rule and the requirement to preserve outside-range paths/follows and
explicit one-play authority. This does not claim the implementation or DP-06
acceptance is complete; [Requirements](REQUIREMENTS.md) tracks the remaining work.

On 2026-11-07 the product owner delegated spec interpretation: the current
specification was drafted without careful review, so the implementing agent
resolves its ambiguities and revises it, reporting each decision. Section 6.5
was revised under that authority. `:audio-shift` slides sound continuously
across cuts (the owner's own choice). A video-only delete removes the topmost
picture layer, trimming a covering cutaway before blanking the beat, and acts
on each beat's part of a range across cuts; Deadpan has no layer beneath a
beat's picture, so "an underlying attachment" became the beat under a cutaway.
A video-only repeat keeps `overflow=trim` as its explicit policy: a repeat never
runs short of its interval, so a hold had nothing to hold, and a fitted speed
change would need a new picture primitive for little creative value.
An audio-only repeat may span cuts: a syllable or phrase rarely respects a
beat boundary, and the repeated sound is the same root-sound construction as
`:audio-shift`. Sound already muted inside the range refuses, because playing
it again would contradict an earlier explicit edit. Video-only repeats span
cuts the same way, one cutaway per beat stretch.

On 2026-11-09 Section 10.5's "variable rate" was defined under the same
delegated authority: a linear ramp of the rate in output time, whose exact
integral fixes every picture and sample position, with the Retime's integral
input range and output duration fixing the mean rate (the authored speeds are
the shape, so one whole-frame rounding of the duration never accumulates).
Linear in output time was chosen because its integral stays a rational
quadratic, so no position needs an irrational or iterative evaluation; named
easings would lose that unless they are polynomial, so none was added. The
stepped form remains a second, explicitly chosen variable-rate construction.
Pitch-preserving continuous ramps stay required and unimplemented.

On 2026-11-10 three Section 8.3 role edits were extended under the same
delegated authority. A bleep across cuts stays one Hold with one tone, the
later passages shown as picture-only cutaways on it, because restarting the
tone per passage would dip at each cut; framed pictures across a cut refuse
since a cutaway carries no framing. A J- or L-cut beside a pause or across a
group boundary moves only sound: the leading beat's handle plays as a root
sound event, Hard at the seam so it stays continuous, while a silent pause gets
an explicit allowance and a beat is muted under it. Rolling a pause would
need media it does not have, and shortening it would change a picture the
user placed. An audio lag on a Source with an editorial window now keeps the
window and fills it with the offset sound from the Original's handle, the
selection rule Trim and Slip already used, so "explicit link offset" means the
sound is slipped against its picture within the same beat rather than leaving
a silent hole at one end; Slip and Trim then carry the offset unchanged.

Also on 2026-11-10, Section 10.5's variable rate gained its pitch-preserving
form under the same delegated authority. The canonical stretcher keeps its
fixed 256-sample call grid and each call consumes the input between the exact
integral's rounded values at its two boundaries, so preview and export share
one schedule and the only approximation is the linear rate inside one call,
bounded by |r1 - r0|·256²/(8N) + 1/2 samples. Changing the rate per sample
or per analysis hop instead would need changes inside the pinned upstream
stretcher. Its speeds stay within the stretcher's 1/8x to 8x rather than the
tape resampler's 1/16x to 16x.

Also on 2026-11-10, Section 10.5's reverse was extended to composite and
speed-changed passages under the same delegated authority. A captured form
would have needed either a stored PCM artifact rendered during command
planning (asynchronous media work inside macros and dot-repeat) or a frozen
audio context, which cannot yet capture the binding state almost every
edited project has. A live reference to the frames before the pause, the
model hanging tails already use, keeps picture and sound consistent with
each other through every later edit, stores no media and needs no
admission. One continuous natural-rate Original passage keeps its captured
form, so existing reverses are unchanged.

Also on 2026-11-10, Section 5.4 gained the anchor semantics of beds and
stings under the same delegated authority, closing DP-09.6. A beat-attached
sting follows its beat's identity through move, split, group, deletion and
Repeat, sounding in every play by default because an attachment inside a
Repeat is evaluated per play; `rib` keeps it on the first play. A root bed
stays on the edit's own clock: moves inside it leave it in place, ripple
deletion removes the deleted part and insertions are silent in it. A bed
drop on such a bed maps the cursor back through its retained route, so
dropping a bed never needs to discard the route or re-time the recipe.

| File | Bytes | SHA-256 |
| --- | ---: | --- |
| [AGENT_HANDOFF.md](spec/archive/1.0/AGENT_HANDOFF.md) | 4,340 | `464f689e1eb5ad6b2905286e516259f07dc83fea8ebb2b0330663976b905ae6c` |
| [DEADPAN_SPEC.md](spec/archive/1.0/DEADPAN_SPEC.md) | 132,459 | `cec6ddc6efddc5dd0b543f05c39834df5243e5629609d8006152a56b4c06adf8` |
| [DEADPAN_SPEC.pdf](spec/archive/1.0/DEADPAN_SPEC.pdf) | 232,612 | `cf25aa245061f5c02c8a8518aa1ebb53903de9536b4c91897faed0f1da23767e` |
| [KEYBOARD_REFERENCE.md](spec/archive/1.0/KEYBOARD_REFERENCE.md) | 3,275 | `4382c26489354ff4856b32a52464a5893d8ef18365e2d13bd8d57821a32aa3e2` |
| [README.md](spec/archive/1.0/README.md) | 1,082 | `80371fcc24220c996765e1decb4e00e95e0e8d5a2f65bbbb931172454bb72a3e` |
