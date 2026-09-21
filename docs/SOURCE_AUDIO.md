# Original audio decoding and indexing

`deadpan-source::audio::AudioDecoder` opens one explicitly selected audio stream
from a private file descriptor. Its persistent software decoder retains raw
frame PTS/DTS, reported duration, original sample rate, channel interpretation,
sample format/count, discard flag and manual skip side data. It returns owned
interleaved f32 samples without resampling, downmixing, gain, clipping or automatic
padding removal. PCM16 conversion is exactly `sample / 32768`.

The tested subset is AAC-LC in MP4 and signed 16-bit little-endian PCM in WAV.
The opening guard admits a strict nonfragmented MP4 grammar and plain RIFF/WAV.
Matroska, fragmented/encrypted/compressed container structures and unqualified
metadata grammars are rejected before FFmpeg parsing. Other codecs, custom/ambisonic layouts, unsupported
sample formats, corrupt frames and changed stream contracts fail explicitly.
Unspecified channel slots remain unspecified; they are not assigned speakers.

Declared packet sizes and table counts are checked before FFmpeg can allocate
from them. The guard uses positional reads with aggregate header, atom, depth,
sample and table budgets across all tracks. WAV packet sizes are capped before
demuxing; payload and side-data sizes are rechecked before codec submission.
The guard has an eight-page, 32 KiB read cache, a 16 MiB aggregate header/read
limit, one million samples and table rows across all tracks, 100000 atoms, 33
tracks and 16 levels of nesting. Header reads and FFmpeg opening share the same
per-call byte allowance and deadline. Interleaved sample tables use the bounded
cache so ordinary index traversal does not repeatedly reread every table page.
The guard preserves original bytes and timing metadata, and relies on the same
immutable-input contract as the native decoder. It does not change FFmpeg's
process-global allocator or serialize otherwise independent decoders.

`VerifiedSourceInput` hashes and copies one complete original into private bytes.
Video and audio sessions can share that snapshot with independent decoder
positions. No path or writable descriptor escapes. Ownership of an ordinary
caller-provided `File` would not establish immutability.

`AudioSession` drains the selected decoder once and writes physical interleaved
PCM to a bounded private temporary file. It builds an identity-bound
`AudioIndexSnapshot`; reads use that cache and do not depend on compressed-audio
seek equivalence. The index stores raw observations and reconstructs derived
offsets on deserialization. Its decoder contract is versioned independently of
authored documents.

Exact indexing currently requires integral original sample coordinates, positive
measured frame durations, and contiguous physical decoded sample positions.
Unsupported gaps, coarse clocks, cross-frame skip counts and contradictory
duration/skip evidence are explicit failures. Within each frame, explicit skip
and discard metadata and the measured frame duration determine available
coverage. Excluded spans remain unavailable. Sample-range reads return an error
for padding, gaps or out-of-range requests; they never substitute silence.

Container duration and codec-parameter padding remain observations. They do not
override measured frame endpoints or establish an editorial origin. The offset
AAC fixture starts at sample 95072 without explicit leading-skip evidence. The
host preserves those samples, including the 1024 samples that independent fixture
provenance identifies as priming. Import must resolve that origin using qualified
evidence before selecting authored media. This index alone is not an import
readiness receipt.

Default session budgets are one million index frames, 1 GiB of PCM cache, 65536
sample frames per read, and a five-minute opening deadline. Native packet, input,
I/O, channel, sample-rate and decoded-sample limits apply as well. Checks precede
snapshot copying and PCM allocation. Deadlines are cooperative; arbitrary
blocking readers and individual native calls are not preemptible. All copying,
decoding, indexing, cache I/O and allocation belong on media service threads,
never UI or real-time audio callbacks.

The PCM cache is disposable and private. The original remains separately owned
by the store. [Source registration](SOURCE_REGISTRATION.md) retains measured
indexes and binds exact independent mappings to qualified selected streams and
their common A/V origin. A source index alone cannot establish that decision.
[Independent destination mapping](SOURCE_AUDIO_MAPPING.md) represents shorter
or delayed audio without changing its natural rate. [Basis state](PRESENTATION_BASIS.md)
locks the clock on timed insertion. Resampling, DSP, device output, listening, scheduling and preview/export
equivalence remain open.

Run `cargo test -p deadpan-source -p deadpan-media --locked` with the qualified
FFmpeg prefix. Verify generated PCM fixtures with
`python3 native/deadpan-source/tests/generate_audio_fixtures.py --verify`.
See [audio source qualification](qualification/source-audio-2026-09-21.md).
