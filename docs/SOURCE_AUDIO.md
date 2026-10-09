# Original audio decoding and indexing

`deadpan-source::audio::AudioDecoder` opens one explicitly selected audio stream
or the first actual admitted audio stream with `open_first`, from a private file
descriptor. `AudioSession::open_first_input` exposes the same choice above the
verified-input boundary. Automatic selection comes from the complete bounded
container guard, including MP4 video at stream 0 and audio at stream 1. It never
guesses indices or repeatedly opens decoders. Explicit selection remains unchanged.
Both paths share header/opening byte and deadline budgets; absence of admitted
audio fails explicitly. Its persistent software decoder retains raw
frame PTS/DTS, reported duration, original sample rate, channel interpretation,
sample format/count, discard flag and manual skip side data. It returns owned
interleaved f32 samples without resampling, downmixing, additional gain, clipping or automatic
padding removal. PCM16 conversion is exactly `sample / 32768`.
Unsigned8 uses `(sample - 128) / 128`; signed24 is left-aligned by FFmpeg into
signed32 and divided by `2^31`, preserving every normalized 24-bit value exactly.
Signed32 uses the same scale and rounds once to f32. Float32 retains every finite
value, including signed zero, subnormals and levels outside ±1. NaN and infinity
fail sample copying and poison the decoder; qualification cannot publish a PCM
cache from them.

The tested subset is AAC-LC in MP4, mono/stereo raw MP3, mono/stereo Opus in MP4 and finite WebM/Matroska,
and unsigned8, signed16/24/32 or float32 PCM in WAV. Opus applies its declared header gain.
The opening guard admits a strict nonfragmented MP4 grammar and qualified RIFF/WAV
with either a plain format header or the closed extensible format described in
[source admission](SOURCE_ADMISSION.md). The extensible form retains its explicit
speaker mask; equal valid/container widths and the PCM/IEEE float subtype are
checked before demuxing. Optional `fact` must match the complete data-frame count.
WAV packet limits use the encoded width and block alignment for every format.
See [wide PCM qualification](qualification/pcm-sources-2026-10-09.md).
Fragmented/encrypted/compressed container structures and unqualified
metadata grammars are rejected before FFmpeg parsing. Other codecs, custom/ambisonic layouts, unsupported
sample formats, corrupt frames and changed stream contracts fail explicitly.
Unspecified channel slots remain unspecified; they are not assigned speakers.
A sound registered from such a file carries the person's explicit speaker
interpretation in its receipt instead; see
[source registration](SOURCE_REGISTRATION.md#explicit-speaker-interpretation).

Declared packet sizes and table counts are checked before FFmpeg can allocate
from them. The guard uses positional reads with aggregate header, atom, depth,
sample and table budgets across all tracks. WAV packet sizes are capped before
demuxing; payload and side-data sizes are rechecked before codec submission.
The MP4 guard has an eight-page, 32 KiB read cache, a 16 MiB aggregate header/read
limit, one million compressed samples and table rows across all tracks, 100000
atoms, 33 tracks and 16 levels of nesting. Matroska and raw MP3 use sparse reads
under the same 16 MiB header-work ceiling. Header reads and FFmpeg opening share the same
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
For AAC/PCM, unsupported gaps, coarse clocks, cross-frame skip counts and contradictory
duration/skip evidence are explicit failures. Within each frame, explicit skip
and discard metadata and the measured frame duration determine available
coverage. Excluded spans remain unavailable. Sample-range reads return an error
for padding, gaps or out-of-range requests; they never substitute silence.

## MP3 framing and sample clocks

Raw MPEG-1/2/2.5 Layer III uses pinned FFmpeg's `mp3float` decoder at its original
rate, with explicit mono or stereo speakers. Its first parsed packet supplies
the stream parameters and remains pending for ordinary decoding; opening never
uses `find_stream_info` or consumes that packet's skip metadata. Physical PTS
and durations use exact `1/14112000` ticks, divisible by all nine MP3 rates.

The `mp3` receipt retains the complete admitted frame inventory, samples per
frame, rate, channels and explicit leading/trailing trim. Every decoded frame,
duration and skip record must agree. LAME/Lavf/Lavc delay and padding are read
from the encoder tag, checked against its checksum and physical frame/byte
counts, and reconciled with FFmpeg's 529-sample synthesis delay. Leading and
trailing trims may each span multiple frames. Untagged audio retains every
physical decoded sample; no guessed priming removal or event alignment runs.
The measured samples, never declared duration, define the endpoint.
See [MP3 qualification](qualification/mp3-sources-2026-10-09.md).

## Opus container and sample clocks

Mono/stereo Opus in finite WebM/Matroska uses pinned libopus 1.6.1, through
FFmpeg 8.0.3, with owned interleaved float output. CELT, SILK and hybrid modes
are exercised. Ogg, multichannel mapping families and other container grammars
remain unqualified. No source-clock behavior is inferred from a file extension.

`matroska_opus` retains the checked pre-skip, nanosecond CodecDelay, timestamp
scale, first raw Block timestamp, packet count and physical sample count.
The first Block names an exact 48 kHz sample; subtracting pre-skip establishes
the physical decode origin. The codec's decoded counts supply subsequent
sample positions. Every raw frame PTS remains in the receipt and must agree
with that origin-based clock within one original tick, capped at one millisecond.
The check reverses only FFmpeg's documented rounding of CodecDelay to ticks.
It never accumulates rounded packet durations, resamples, inserts silence,
drops packets or aligns to audio events. Coarse metadata cannot distinguish a
deliberate gap inside that tick envelope from quantization; this is an explicit continuity
interpretation, not a claim that raw timestamps have sample precision.

Raw reported durations must agree with physical or explicitly trimmed packet
duration within that same tick. Exact leading pre-skip may span packets;
terminal DiscardPadding must represent whole samples within strictly less than
one nanosecond and removes only that declared count. Extra leading skips,
interior trailing skips, discard flags, mismatched counts or drift outside the
tick envelope fail. Deserialization reconstructs and rechecks every coordinate.
The final endpoint uses physical samples and explicit trims, never the rounded
container duration. See the [qualification](qualification/opus-sources-2026-10-09.md).

## MP4 Opus sample clocks

Mono/stereo Opus in nonfragmented MP4 uses the same pinned libopus decoder.
The [Opus ISO BMFF mapping](https://opus-codec.org/docs/opus_in_isobmff.html)
defines the `Opus` entry, big-endian `dOps` fields, roll recovery and presentation
edits. Deadpan admits the qualified grammar in [source admission](SOURCE_ADMISSION.md).

The separate `mp4_opus` receipt retains pre-skip, channels, physical packet/sample
counts, the first exact sample position and the valid presented count. Every
decoded packet must match that inventory. PTS uses exact `1/48000` ticks;
there is no Matroska quantization allowance. Raw discard and skip evidence must
match the admitted edit, including wholly skipped priming packets. The final
reported duration may be shorter than the physical decoded packet and defines
its available endpoint. Header duration alone never supplies PCM coverage.

The eleven retained fixtures cover CELT, SILK, hybrid, 2.5/20/60/120 ms packets,
mono/stereo, header gain, cross-packet priming and a 6,000-sample offset. Their
compressed audio packets match the retained WebM corpus; independent reference
PCM checks every available sample. H.264, AV1 and HDR VP9 picture copies retain
their exact decoded pictures and timestamps. Receipt loading rechecks every
coordinate and rejects altered counts, offsets, skips and durations. See
[qualification](qualification/opus-mp4-2026-10-09.md).

## Shared indexing and cache limits

For AAC/PCM, container duration and codec-parameter padding remain observations. They do not
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
