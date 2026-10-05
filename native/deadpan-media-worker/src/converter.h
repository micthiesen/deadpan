#ifndef DEADPAN_MEDIA_CONVERTER_H
#define DEADPAN_MEDIA_CONVERTER_H

#include <stddef.h>
#include <stdint.h>

typedef struct {
    uint32_t width;
    uint32_t height;
    uint32_t frames;
    uint32_t rate_num;
    uint32_t rate_den;
    uint64_t input_byte_length;
    uint64_t max_input_bytes;
    uint64_t max_output_bytes;
    uint64_t max_scratch_bytes;
    uint64_t timeout_ms;
    uint32_t output_frames;
    uint32_t output_rate_num;
    uint32_t output_rate_den;
    uint32_t sample_bridge;
} DeadpanConversionRequest;

typedef struct {
    uint64_t output_bytes;
    char input_rgb_sha256[65];
    char output_rgb_sha256[65];
    uint32_t input_time_base_num;
    uint32_t input_time_base_den;
    uint32_t output_time_base_num;
    uint32_t output_time_base_den;
    int64_t first_output_pts;
    int64_t last_output_pts;
    int64_t last_output_duration;
    uint32_t ffv1_version;
    uint8_t slice_crc;
    uint32_t discarded_audio_streams;
} DeadpanConversionReport;

typedef struct {
    char code[48];
    char message[256];
} DeadpanConversionError;

int deadpan_convert(int input_fd, int output_fd, int scratch_fd,
                    const DeadpanConversionRequest *request,
                    DeadpanConversionReport *report,
                    DeadpanConversionError *error);

typedef struct {
    uint64_t video_byte_length;
    uint64_t audio_byte_length;
    uint64_t max_output_bytes;
    uint64_t timeout_ms;
} DeadpanRemuxRequest;

typedef struct {
    uint64_t output_bytes;
    uint64_t video_packets;
    uint64_t audio_packets;
    uint32_t width;
    uint32_t height;
    uint32_t sample_rate;
    uint32_t channels;
} DeadpanRemuxReport;

/* The input descriptor holds the picture input followed immediately by the
   sound input. */
int deadpan_remux(int input_fd, int output_fd, const DeadpanRemuxRequest *request,
                  DeadpanRemuxReport *report, DeadpanConversionError *error);

/* Preview proxy: an intra-only VideoToolbox H.264 MP4 at a reduced raster.
   Open, push every Original picture as straight RGBA in presentation order
   with its exact PTS and duration in the Original time base, then finish.
   One proxy per process; every failure releases the encoder. */
typedef struct {
    uint32_t source_width;
    uint32_t source_height;
    uint32_t width;
    uint32_t height;
    uint32_t time_base_num;
    uint32_t time_base_den;
    uint32_t sar_num;
    uint32_t sar_den;
    uint32_t rotation_quarter_turns;
    uint32_t transfer;
    uint32_t primaries;
    uint32_t quality;
    uint64_t frames;
    uint64_t max_output_bytes;
    uint64_t timeout_ms;
} DeadpanProxyRequest;

typedef struct {
    uint64_t output_bytes;
    uint64_t packets;
    uint64_t keyframes;
    uint32_t width;
    uint32_t height;
} DeadpanProxyReport;

int deadpan_proxy_open(int output_fd, const DeadpanProxyRequest *request,
                       DeadpanConversionError *error);
int deadpan_proxy_push(const uint8_t *rgba, uint64_t rgba_bytes, uint64_t stride, int64_t pts,
                       int64_t duration, DeadpanConversionError *error);
int deadpan_proxy_finish(int output_fd, DeadpanProxyReport *report,
                         DeadpanConversionError *error);

#endif
