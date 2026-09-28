# Gain recipes and structural clocks, 2026-09-27

This increment implements standalone gain recipes and exact structural owner
queries for the next authored-audio work. It does not change PCM, documents,
database schemas or the native workspace. DP-09 and all release gates remain
open or partial. The full integration boundary is in [Audio gain](../AUDIO_GAIN.md).

## Environment and scope

- Base: `186ec3a6afe81390045f1e5005518bb9d881aa6e`.
- Apple M5 Max, 128 GiB, macOS 26.5.2 build 25F84.
- Rust 1.97.1 (`8bab26f4f`, 2026-07-14), locked workspace dependencies.
- Native dependency prefix: `/tmp/deadpan-ui-ffmpeg/prefix`.
- New core gain modules/tests; new plan owner-query modules/tests and narrow
  exports; design boards/prompts, contracts and handoff updates.

The [retained records](../../tools/audio-qualification/evidence/2026-09-27-gain-clocks/summary.json)
identify each command, terminal exit, duration and exact build-source manifest.
Compressed logs and source manifests preserve failures as well as passing runs.
The evidence manifest hashes every retained record; design assets have their
own [manifest](../design/manifest.json). Logs do not establish capability beyond
the assertions and boundary described here.

The final build-source manifest is
`c043a7401dc1d7551aa5e2eaaf027a5e6507a0adf12242f7ab366fd4c173c194`.
Formatting and strict workspace/all-targets Clippy passed. Focused verification
passed 14 core gain integration tests, 14 owner tests after the Preserve fix,
and the subsequently added selected-Source regression. The workspace suite
passed 1973 tests across 154 result groups, with zero failures and zero ignored
tests, including the three private numeric tests and existing consumers.
The single workspace test command took 2173.87 seconds including compilation;
strict Clippy took 649.32 seconds. Several media-heavy tests emitted Rust's
60-second notice and subsequently passed. Neither run was restarted. These
durations are verification costs, not editor latency measurements.

## Independent review and corrections

An independent reviewer inspected numeric bounds, ingress, owner namespaces,
binding clocks and work limits. The review found and closed:

1. Serde's internally tagged unit curves accepted surplus fields despite the
   enum's `deny_unknown_fields`. The first focused integration run failed its
   existing malformed-curve assertion. A closed flat map visitor now rejects
   unknown keys, duplicates and invalid controls before consuming their values.
2. Internally tagged enum decoding buffered arbitrarily large malformed values
   before validation. The streaming visitor has early-rejection tests with
   deeply nested, unterminated payloads. Standalone recipe JSON is capped at
   512 KiB before parsing and during writing. Direct Serde embedding still
   requires byte-bounded caller input; global exact-ratio vocabulary is unchanged.
3. An outer crop incorrectly constrained the retained input support of an inner
   Preserve stage. The owner traversal now keeps outer delivery allocation
   separate from full intrinsic Preserve input support. Regression cases use
   the reviewer's cropped/resumed example and an enclosing Preserve.

The reviewer re-inspected the fixes and reported no remaining concrete finding.
The parent inspected the implementation and review corrections and added the
selected-Source case, with an exact selected interval, signed audio offset, resumed half-sample
phase, suffix query and explicit exhausted-support rejection.

## What the tests establish

The core integration cases cover all four curves, independent dB interpolation
oracles, half-open boundaries, right-continuous keys, sample-derived coordinates,
full-width rational products, non-dyadic boundary neighbors, Q32 tie behavior,
overlapping envelopes, exact mute, preserved trim and bounded typed/JSON ingress.
Private numeric tests exercise six-limb carry/borrow/overflow and independent
integer rounding oracles.

The plan cases cover distinct outer/inner clocks, stable Repeat and default-gap
identities, sparse play/gap overrides, nested Preserve/FollowSpeed, transparent
partitions, independently rounded NTSC origins, retained odd-sample bindings,
intrinsic PointCeil input, physical/definition namespaces and plan branding.
The same small nested fixture is queried whole, in irregular out-of-order blocks
and one sample at a time; exact owner identity, provenance, coordinates and
slopes agree. A far play in a billion-play Repeat stays under bounded work.
Work/span exhaustion returns an error instead of a partial successful answer.

Owner queries deliberately reject an exhausted retained support interval when
they cannot establish its clock. This is an explicit remaining integration
boundary, not a unity-gain fallback or proof of complete treated-bus rendering.

## Visual review and remaining work

The built-in imagegen tool generated [v1](../design/boards/clip-gain-board-v1.png)
and the corrected [v2 target](../design/boards/clip-gain-board-v2.png). Both exact
prompts and original outputs are retained in the repository. Full-size output
inspection checked picture priority, focused-field visibility, independent trim
versus envelope/mute, Before/Draft intent and readable key labels. The correction
aligns all clocks with frame 246 at 30 fps and clarifies field/key ownership.
The graph and waveform are representative design content, not measured media.

No native UI code changed. Under the backend-only development convention, this
increment does not repeat optional `ui-harness` feature checks, GUI replay,
physical input, VoiceOver, acoustic listening or release-performance checks.
The board must be compared against the implemented native editor when that work
lands. Persisted node treatments/commands, migration, full PCM processing,
cache/admission behavior, temporary audition identity, waveform service,
keyboard routing and preview/export equivalence remain required.
