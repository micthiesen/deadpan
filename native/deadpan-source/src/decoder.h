#ifndef DEADPAN_SOURCE_DECODER_H
#define DEADPAN_SOURCE_DECODER_H
#include <stdint.h>
#include <stddef.h>

typedef struct DeadpanSource DeadpanSource;
typedef int (*DeadpanCancelled)(const void *);
#define DEADPAN_SOURCE_MAX_AUDIO_STREAMS 32
typedef struct {
    uint64_t max_input_bytes;
    uint64_t max_frames;
    uint64_t max_packets;
    uint64_t max_io_bytes_per_call;
    uint64_t max_packet_bytes;
    uint64_t max_pixels;
    uint32_t max_dimension;
    uint32_t max_packets_per_frame;
    /* Decoder worker threads, 1..16. More than one enables FFmpeg's
       deterministic frame/slice threading; output is unchanged. */
    uint32_t threads;
} DeadpanSourceLimits;
typedef struct {
    int32_t stream_index;
    int32_t time_base_num, time_base_den;
    int64_t stream_start, stream_duration;
    int32_t sample_rate, channel_count;
    char codec[32];
} DeadpanSourceAudioInfo;
typedef struct {
    int32_t width, height;
    int32_t stream_index;
    int32_t time_base_num, time_base_den;
    int32_t sar_num, sar_den;
    int32_t rotation;
    int32_t range, matrix, transfer, primaries;
    int64_t stream_start, stream_duration, container_start, container_duration;
    char codec[32];
    char pixel_format[32];
    uint32_t audio_stream_count;
    DeadpanSourceAudioInfo audio_streams[DEADPAN_SOURCE_MAX_AUDIO_STREAMS];
    /* SMPTE ST 2086 / CTA-861.3 static metadata, admitted only with a PQ or
       HLG transfer. Primaries R,G,B and white point in 1/50000 CIE xy units;
       luminance in 1/10000 cd/m2; content light in cd/m2. has_* is 0 when
       absent, 1 when exactly representable in these fields and 2 when declared
       but not representable (the host ignores it with a recorded note). */
    int32_t has_mastering;
    uint16_t mastering_primaries[3][2];
    uint16_t mastering_white_point[2];
    uint32_t mastering_max_luminance, mastering_min_luminance;
    int32_t has_content_light;
    uint16_t max_cll, max_fall;
    /* Pinned BWDIF send_field interpretation; output ticks are half source
       ticks, including stream_start/duration. Audio clocks are unchanged. */
    int32_t bwdif_fields;
} DeadpanSourceInfo;
typedef struct {
    int64_t pts, duration, dts;
    int32_t keyframe;
} DeadpanSourceFrame;
typedef struct {
    DeadpanSourceFrame source;
    int64_t best_effort_pts;
    int32_t picture_type, decoder_profile, codec_profile, chroma_location;
    int32_t stream_sar_num, stream_sar_den, codec_sar_num, codec_sar_den;
    int32_t frame_sar_num, frame_sar_den;
    int32_t flags, decode_error_flags;
    int32_t interlaced, top_field_first, corrupt;
} DeadpanExportFrame;
typedef struct {
    uint64_t frames, packets, io_bytes;
    /* Picture buffers the codec allocated: pictures actually decoded,
       including preroll that is never returned. Skipped pictures add none. */
    uint64_t pictures;
} DeadpanDecodeWork;
typedef struct {
    uint32_t avcodec, avformat, avutil, swscale, avfilter;
} DeadpanDecoderRuntime;
typedef struct {
    char code[48];
    char message[256];
} DeadpanSourceError;
int deadpan_source_open(int fd, int64_t length, const DeadpanSourceLimits *limits,
    uint64_t preflight_io_bytes, uint64_t timeout_ms, DeadpanCancelled cancelled, const void *opaque,
    DeadpanSource **out, DeadpanSourceInfo *info, DeadpanSourceError *error);
int deadpan_source_open_at_keyframe(int fd, int64_t length, const DeadpanSourceLimits *limits,
    uint64_t preflight_io_bytes, uint64_t timeout_ms, DeadpanCancelled cancelled, const void *opaque,
    DeadpanSource **out, DeadpanSourceInfo *info, DeadpanSourceError *error, int64_t pts);
int deadpan_source_next(DeadpanSource *source, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanSourceFrame *frame,
    uint8_t *rgba, size_t rgba_length, DeadpanSourceError *error);
int deadpan_source_copy(DeadpanSource *source, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanSourceFrame *frame,
    uint8_t *rgba, size_t rgba_length, DeadpanSourceError *error);
/* Like next/copy, but packed little-endian RGBA64 (u16 per channel, full
   RGB range, alpha 65535) with the same explicit matrix/range/chroma siting. */
int deadpan_source_next_rgba64(DeadpanSource *source, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanSourceFrame *frame,
    uint8_t *rgba, size_t byte_length, DeadpanSourceError *error);
int deadpan_source_copy_rgba64(DeadpanSource *source, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanSourceFrame *frame,
    uint8_t *rgba, size_t byte_length, DeadpanSourceError *error);
int deadpan_source_seek(DeadpanSource *source, int64_t pts, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanSourceError *error);
/* Seek like deadpan_source_seek, then skip decoding non-reference pictures
   whose packet PTS precedes target_pts. Those pictures are never returned;
   every returned picture is bit-identical to an ordinary forward decode. */
int deadpan_source_seek_to(DeadpanSource *source, int64_t pts, int64_t target_pts, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanSourceError *error);
int deadpan_source_restart_at_keyframe(DeadpanSource *source, int64_t pts, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanSourceError *error);
int deadpan_source_next_i420(DeadpanSource *source, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanExportFrame *frame,
    uint8_t *i420, size_t i420_length, DeadpanSourceError *error);
int deadpan_source_copy_i420(DeadpanSource *source, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanExportFrame *frame,
    uint8_t *i420, size_t i420_length, DeadpanSourceError *error);
/* Tight planar Y, Cb, Cr decoded 10-bit samples of limited-range BT.2020
   NCL PQ/HLG yuv420p10le pictures with even dimensions; no conversion. */
int deadpan_source_next_yuv420p10(DeadpanSource *source, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanExportFrame *frame,
    uint16_t *samples, size_t sample_count, DeadpanSourceError *error);
int deadpan_source_copy_yuv420p10(DeadpanSource *source, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanExportFrame *frame,
    uint16_t *samples, size_t sample_count, DeadpanSourceError *error);
void deadpan_source_work(const DeadpanSource *source, DeadpanDecodeWork *work);
void deadpan_source_runtime(DeadpanDecoderRuntime *runtime);
void deadpan_source_close(DeadpanSource *source);
/* Lower the calling thread to utility QoS (macOS); codec threads it creates
   afterwards inherit it. Returns 1 on success, 0 where unsupported. */
int deadpan_source_lower_thread_priority(void);
#endif
