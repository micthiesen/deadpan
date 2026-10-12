# Saved stretched audio

Native playback, idle preparation and the headless project audio session can
reuse canonical Preserve and integer-pitch stage output from
`~/Library/Caches/Deadpan/Audio`. Ordinary and projected continuous stages use
the same cache. It contains derived stereo f32 PCM, never originals, accepted
AI media, project state or history. Hold-specific reverse and room-tone blocks
remain private to their audio engine.

A lookup scans the complete freshly admitted stage input in bounded windows.
Its BLAKE3 key includes actual canonical sample bits, exact source mappings and
selections, transitive source content/interpretation fingerprints, exact rate,
pitch, input/output extents and the output silence mask. A compiled source pin
covers the audio, plan, core, media, DSP and decoder sources, locked dependencies,
toolchain, target, profile, target features and configured compiler flags. Nested revision-bearing plan
descriptors can cause conservative misses. Cached metadata never admits a source.
Cold misses scan input again for the canonical processor; this costs extra work.

Restoration checks the header, key, exact extent, every finite sample and the
complete BLAKE3 payload checksum while copying into private prepared PCM. Only
the completed verified copy reaches playback or export. Damaged, busy, full or
unavailable caches fall back to canonical preparation. Source failures,
cancellation, work limits and deadlines still fail explicitly. An active private
copy continues to work after its persistent entry is removed.

Publication reserves the full logical file size under a nonblocking cache lock,
writes a unique staging file, syncs it and atomically renames it. The shared
budget is 4 GiB, including concurrent staging files, with at most 4,096 entries
and 86,400,000 stereo frames per entry. Existing stage RAM, private-file, query
work and deadline bounds remain separate. Full caches keep private output and
report the miss instead of evicting recently used entries. Cache I/O stays on
preparation/storage workers, away from the UI and audio callback.

Persistent-key scans share one extra allowance of 16,777,216 plan-work units
per preparation request, across nested scans. They cannot spend the canonical
render's own allowance. Exhaustion skips the lookup and prepares directly,
with a visible diagnostic. Source admission, prepared-stage/frame counts, RAM,
disk and the original deadline remain shared; a lookup never renews them.

Reads hold a shared file lock; publication and cleanup require exclusive locks.
Descriptor-relative operations reject symlinks, hardlinks, unsafe permissions,
foreign owners, nonregular files and replaced roots. Cleanup skips active files.
Successful restoration updates last-use time. Automatic publication cleanup
removes entries unused for 30 days; explicit Storage/cache cleanup uses its
configured grace, at least one hour. Abandoned staging obeys the same grace.

`:storage` reports saved keys, sizes, origin and validity. Listing says
“payload checked on restore” because it has not verified every payload.
The panel shows up to 32 entries; `cache status` lists the bounded complete set.
`:diagnostics` reports the current request's last saved/restored stage, full key
and rebuild reason. Existing PCM file counters include persistent payload
reads/writes; headers and filesystem metadata are not counted.
Preparation status distinguishes input checking, restoration and saving.

Qualification is recorded in [persistent audio](qualification/persistent-audio-2026-10-11.md).
The remaining voice, group, mastering and device/export corpus keeps DP-09 and
DP-16 partial.
