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
} DeadpanDecodeWork;
typedef struct {
    uint32_t avcodec, avformat, avutil, swscale;
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
int deadpan_source_seek(DeadpanSource *source, int64_t pts, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanSourceError *error);
int deadpan_source_restart_at_keyframe(DeadpanSource *source, int64_t pts, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanSourceError *error);
int deadpan_source_next_i420(DeadpanSource *source, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanExportFrame *frame,
    uint8_t *i420, size_t i420_length, DeadpanSourceError *error);
int deadpan_source_copy_i420(DeadpanSource *source, uint64_t timeout_ms,
    DeadpanCancelled cancelled, const void *opaque, DeadpanExportFrame *frame,
    uint8_t *i420, size_t i420_length, DeadpanSourceError *error);
void deadpan_source_work(const DeadpanSource *source, DeadpanDecodeWork *work);
void deadpan_source_runtime(DeadpanDecoderRuntime *runtime);
void deadpan_source_close(DeadpanSource *source);
#endif
