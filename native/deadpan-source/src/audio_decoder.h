#ifndef DEADPAN_AUDIO_DECODER_H
#define DEADPAN_AUDIO_DECODER_H
#include <stddef.h>
#include <stdint.h>
#include "decoder.h"
typedef struct DeadpanAudio DeadpanAudio;
typedef struct {
    uint64_t max_input_bytes, max_frames, max_packets, max_decoded_samples;
    uint64_t max_io_bytes_per_call;
    uint32_t max_samples_per_frame, max_channels, max_sample_rate;
    uint32_t max_packets_per_frame, max_packet_bytes;
} DeadpanAudioLimits;
typedef struct {
    int32_t channels, order;
    uint64_t mask;
} DeadpanAudioLayout;
typedef struct {
    int32_t stream_index, time_base_num, time_base_den, sample_rate;
    DeadpanAudioLayout channel_layout;
    int64_t stream_start, stream_duration;
    int32_t initial_padding, trailing_padding, seek_preroll, sample_format;
    char codec[32];
} DeadpanAudioInfo;
typedef struct {
    int64_t pts, duration, dts;
    int32_t nb_samples, sample_rate, sample_format;
    DeadpanAudioLayout channel_layout;
    uint32_t skip_present, leading, trailing, leading_reason, trailing_reason, discard;
} DeadpanAudioFrame;
int deadpan_audio_open(int fd, int64_t length, uint32_t selected_stream,
    const DeadpanAudioLimits *limits, uint64_t preflight_io_bytes, uint64_t timeout, DeadpanCancelled cancelled,
    const void *opaque, DeadpanAudio **out, DeadpanAudioInfo *info, DeadpanSourceError *error);
int deadpan_audio_next(DeadpanAudio *source, uint64_t timeout, DeadpanCancelled cancelled,
    const void *opaque, DeadpanAudioFrame *frame, DeadpanSourceError *error);
int deadpan_audio_copy(DeadpanAudio *source, uint64_t timeout, DeadpanCancelled cancelled,
    const void *opaque, DeadpanAudioFrame *frame, float *samples, size_t length,
    DeadpanSourceError *error);
void deadpan_audio_close(DeadpanAudio *source);
#endif
