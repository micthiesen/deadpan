# macOS audio output dependency audit, 2026-09-21

**Status: source-audited experimental boundary; release callback contract open.**
CPAL 0.18.2 and rtrb 0.4.0 are suitable candidates for a narrow prepared-PCM
output adapter. This record qualifies neither physical playback nor all upstream
fault paths. It contains no device measurements, listening result, fault
injection, or suspend/resume test. The normative requirement remains
[specification section 16.5](../spec/DEADPAN_SPEC.md#165-realtime-audio).

## Pins and evidence

The inspected code came from the crates.io archives selected by `Cargo.lock`.
All 22 archives in the macOS normal/build dependency closure matched their
locked SHA-256 checksums. `cargo tree -p deadpan-output --locked --target TARGET
--edges normal,build --prefix none --format '{p}'` returned the same package and
version set for `aarch64-apple-darwin` and `x86_64-apple-darwin`. This comparison
does not establish equivalent device behavior on both architectures.

| Package | Archive SHA-256 | Upstream source revision |
| --- | --- | --- |
| CPAL 0.18.2 | `6f02e8d0327b42d3e2e4ab2119af397344eb9fc54a34bf0ddeaa1277af8681f1` | `e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7` |
| coreaudio-rs 0.14.2 | `7d5d7dca3ebcf65a035582c9ad4385371a9d9ee6537474d2a278f4e1e475bb58` | `f07c02c7419328650112d4b276cffd785b0be4ee` |
| rtrb 0.4.0 | `9278fb35b3e730abe136e9b395b5b81b96d06b9f5478a50f0c8430a2237b22de` | `d1f3a2eb042602bcf2a2be2fef195a0a7dfb98af` |

The source revisions are recorded in each archive's `.cargo_vcs_info.json`.
CPAL's `src/host/coreaudio/macos/device.rs`, coreaudio-rs's
`src/audio_unit/render_callback.rs`, and rtrb's `src/lib.rs` were also downloaded
from those exact upstream revisions and compared byte-for-byte with the archive
contents. They matched. Source links below use those revisions, not moving
branches. The full resolved package, checksum and license inventory and copied
notices are in [THIRD_PARTY.md](../../native/deadpan-output/THIRD_PARTY.md) and
[its provenance manifest](../../native/deadpan-output/licenses/manifest.json).

## Callback work and error paths

The normal CPAL macOS output callback accesses the configured interleaved
buffer, fills silence, constructs stack `Data` and timestamp values, loads one
atomic latency value, and calls the supplied closure. It does not acquire the
stream mutex or allocate Rust heap storage on this path. The lower wrapper
constructs stack arguments and invokes a boxed closure allocated during setup;
`data::Raw` only retains the supplied pointer. These conclusions concern the
output path. The separate input implementation has different buffering behavior.
See CPAL [device.rs:872-930][cpal-output], coreaudio-rs
[render_callback.rs:79-91][raw-buffer], [475-543][render-wrapper] and
[766-782][native-entry].

CPAL wraps the error closure in a setup-time `Arc<Mutex<_>>`. Real-time render
errors and processor-overload notifications use `try_lock`, not a waiting lock.
When the lock is contended, the error is returned to the caller and discarded.
Other notifications reach the same closure from delivery threads using a
blocking lock. Therefore the application error closure must be safe on the
real-time thread, and error counts cannot be treated as a complete hardware
fault ledger. Publish a compact atomic fault state; do not log, format, stop,
rebuild or drop a stream from either callback. Xrun construction itself contains
no owned string. See [error_emit.rs:11-39][error-emit] and
[macos/mod.rs:183-228][disconnect].

There is an exceptional allocation before the application callback:
`host_time_to_stream_instant` calls `mach_timebase_info`; a nonzero return passes
through `check_os_status` to `From<coreaudio::Error>`, which calls `format!`.
Passing the resulting owned error into the application's closure can also free
that string when the closure returns. The overflow error in the same conversion
uses a borrowed static message. See [coreaudio/mod.rs:30-31 and 68-84][clock].
This audit found no ordinary disconnect or xrun trigger for the allocating
branch. Apple's published [clock trap][apple-clock] returns `KERN_SUCCESS`, and
its [userspace wrapper][apple-timebase] caches the timebase and returns success.
Those published sources explain the narrow inference; they are not a verified
binary identity or a fault-path qualification for the installed OS.

Consequently stock CPAL cannot be described here as allocation-free for every
possible callback return path. A small upstream change could use a borrowed
static error for this particular status failure, with a regression test for
the failure branch. Validating and capturing the clock ratio during setup is
another possible change. Neither change is part of this audit. An allocator
guard entered only inside our supplied closure does not observe CPAL work before
entry or after return. No claim is made about allocation, contention or hard
execution deadlines inside Apple's proprietary audio implementation.

## Configuration and timestamps

The initial candidate boundary is an explicitly pinned device whose current
advertised configuration is stereo F32 at 48 kHz, with `BufferSize::Default`.
Reject other configurations until they have a deliberate conversion and channel
policy. CPAL configuration enumeration does not establish channel semantics or
bit-perfect output. Callback buffer length is the actual slice length and must
be handled without resizing callback-owned storage.

Even this conservative selection is not a promise of no hardware configuration
mutation. CPAL attempts to set the physical stream format during construction;
on failure it attempts the nominal sample rate, and the AudioUnit may bridge
remaining format differences. The default configuration describes the virtual
stream format, so an already-matching default does not prove that the physical
format attempt is inert. `BufferSize::Fixed` additionally sets a device-wide
buffer property; `Default` omits that setter. Record and revalidate the observed
configuration after opening. The optional build timeout bounds the rate-change
wait, not every CoreAudio call or monitor initialization. See
[device.rs:837-870][format-attempt], [92-222][rate-setting],
[623-672][default-config], and [994-1039][buffer-setting].

Callback timestamps come from CoreAudio's `AudioTimeStamp.mHostTime`, converted
through the Mach timebase. CPAL predicts playback time by adding configured-rate
duration for device-buffer depth, safety offset and device latency. Unknown
buffer depth falls back to the actual callback frame count. Playback timestamps
are clamped nondecreasing and may repeat when the estimate decreases. They are
not measured DAC timestamps, project-sample delivery acknowledgements, or a
calibrated end-to-end latency measurement. Keep submitted frame accounting,
underrun silence and host timestamp observations distinct. See
[device.rs:906-924][timestamp-construction] and
[host/mod.rs:242-297][timestamp-clamp].

## Routing and stream lifetime

`host.default_output_device()` produces a `DefaultOutput` AudioUnit that follows
the system default automatically. Its monitor reports the change after the
unit has rerouted, so an error callback cannot guarantee silence before audio
reaches a replacement endpoint. The monitor refreshes latency and overload
listeners, but does not install the pinned path's nominal-rate and alive
listeners for each default endpoint. Resolve the chosen default device's ID
through `device_by_id()` before building the initial adapter. Enumeration returns
an ordinary device and selects the pinned HAL output path. See
[enumerate.rs:75-79 and 113-140][enumeration], [traits.rs:67-71][lookup],
[device.rs:851-855][format-attempt], and [macos/mod.rs:255-364][default-monitor].

The pinned path registers device-alive and nominal-rate notifications, delivered
through a channel to a monitor thread. It tries to pause using `try_lock`; a busy
stream mutex means that automatic pause is skipped. Our controller must fault,
mute, invalidate the transport generation, and explicitly stop or rebuild off
the callback. A changed default device need not disconnect a still-live pinned
device, so following the user's default requires a separate control-thread
check. Polling does not promise instantaneous detection. Physical headphone
disconnect, same-device configuration changes and suspend/resume still require
real-device tests and explicit reset policy. See [macos/mod.rs:127-246][disconnect].

Construction initializes the AudioUnit and returns it paused. `play()` and
`pause()` are idempotent with respect to CPAL's `playing` flag and take the
stream mutex. `buffer_size()` also locks and queries native state. They belong
on the controller. Pause does not flush our queued PCM or establish a new seek
generation. Drop signals monitor shutdown, while the native destructor stops,
uninitializes, frees callbacks and disposes the AudioUnit, ignoring cleanup
errors. Monitor threads are not joined; an upgraded weak reference can briefly
defer final destruction to a delivery thread. Explicitly pause before dropping
on a non-callback thread and do not describe Drop as a synchronous monitor join.
See [macos/mod.rs:374-453][lifecycle] and coreaudio-rs
[audio_unit/mod.rs:378-395][unit-drop].

## SPSC ownership and remaining qualification

rtrb allocates fixed storage during construction. `push` and `pop` use bounded
loads/stores with acquire/release publication and return immediately on full or
empty, without allocation, a mutex or a retry loop. Each endpoint is `Send`
when its element is `Send`, but is not `Sync`. Move exactly one producer to
preparation and one consumer to the sequential output closure. Do not share one
producer between data and error callbacks. Use bounded plain frame or fixed-size
block values, because arbitrary element destructors can perform heap work.
See [lib.rs:145-153, 292-343, 477-491, 517-579, 754-769][queue].

The second endpoint to be destroyed drops remaining elements and deallocates
the ring storage. Keep endpoint destruction and replacement off the callback;
do not implement seek by replacing its queue there. Empty output must become
silence immediately, with bounded bookkeeping. Generation-based stale-frame
discard also needs a fixed per-callback work limit. See
[arc_ring_buffer.rs:46-85][queue-drop] and [lib.rs:230-245][queue].

This supports implementing and measuring the narrow normal-path adapter. Release
qualification remains open for complete callback error behavior, callback
deadline and allocation measurements, underrun recovery, device changes,
suspend/resume, audible seek/stop behavior, long-run clock drift, actual output
latency, transport/video synchronization and the signed packaged application.

[cpal-output]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/macos/device.rs#L872-L930
[raw-buffer]: https://github.com/RustAudio/coreaudio-rs/blob/f07c02c7419328650112d4b276cffd785b0be4ee/src/audio_unit/render_callback.rs#L79-L91
[render-wrapper]: https://github.com/RustAudio/coreaudio-rs/blob/f07c02c7419328650112d4b276cffd785b0be4ee/src/audio_unit/render_callback.rs#L475-L543
[native-entry]: https://github.com/RustAudio/coreaudio-rs/blob/f07c02c7419328650112d4b276cffd785b0be4ee/src/audio_unit/render_callback.rs#L766-L782
[error-emit]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/error_emit.rs#L11-L39
[clock]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/mod.rs#L30-L84
[apple-clock]: https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/osfmk/kern/clock.c#L375-L392
[apple-timebase]: https://github.com/apple-oss-distributions/xnu/blob/f6217f891ac0bb64f3d375211650a4c1ff8ca1ea/libsyscall/wrappers/mach_timebase_info.c#L26-L45
[format-attempt]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/macos/device.rs#L837-L870
[rate-setting]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/macos/device.rs#L92-L222
[default-config]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/macos/device.rs#L623-L672
[buffer-setting]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/macos/device.rs#L994-L1039
[timestamp-construction]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/macos/device.rs#L906-L924
[timestamp-clamp]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/mod.rs#L242-L297
[enumeration]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/macos/enumerate.rs#L75-L140
[lookup]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/traits.rs#L67-L71
[default-monitor]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/macos/mod.rs#L255-L364
[disconnect]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/macos/mod.rs#L127-L246
[lifecycle]: https://github.com/RustAudio/cpal/blob/e1612d5d98152f8dc2a62e1b51ef7cbf4f7f26b7/src/host/coreaudio/macos/mod.rs#L374-L453
[unit-drop]: https://github.com/RustAudio/coreaudio-rs/blob/f07c02c7419328650112d4b276cffd785b0be4ee/src/audio_unit/mod.rs#L378-L395
[queue]: https://github.com/mgeier/rtrb/blob/d1f3a2eb042602bcf2a2be2fef195a0a7dfb98af/src/lib.rs
[queue-drop]: https://github.com/mgeier/rtrb/blob/d1f3a2eb042602bcf2a2be2fef195a0a7dfb98af/src/arc_ring_buffer.rs#L46-L85
