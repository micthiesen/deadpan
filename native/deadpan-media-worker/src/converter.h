#ifndef DEADPAN_MEDIA_CONVERTER_H
#define DEADPAN_MEDIA_CONVERTER_H

#include <stddef.h>
#include <stdint.h>

typedef enum {
    DEADPAN_SAMPLING_COPY = 0,
    DEADPAN_SAMPLING_BRIDGE_INTERIOR = 1,
    DEADPAN_SAMPLING_EXTENSION_FROM_LEFT = 2,
    DEADPAN_SAMPLING_EXTENSION_FROM_RIGHT = 3
} DeadpanSamplingKind;

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
    /* Integer ABI field: validate before interpreting as DeadpanSamplingKind. */
    uint32_t sampling_kind;
    uint32_t generated_start;
    uint32_t generated_frames;
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
    /* The movie starts at this offset of the output descriptor, which must
       be the descriptor's current length. */
    uint64_t output_offset;
} DeadpanProxyRequest;

typedef struct {
    uint64_t output_bytes;
    uint64_t packets;
    uint64_t keyframes;
    uint32_t width;
    uint32_t height;
    /* SHA-256 of the decoder configuration read back from the finished movie. */
    uint8_t extradata_sha256[32];
} DeadpanProxyReport;

int deadpan_proxy_open(int output_fd, const DeadpanProxyRequest *request,
                       DeadpanConversionError *error);
int deadpan_proxy_push(const uint8_t *rgba, uint64_t rgba_bytes, uint64_t stride, int64_t pts,
                       int64_t duration, DeadpanConversionError *error);
int deadpan_proxy_finish(int output_fd, DeadpanProxyReport *report,
                         DeadpanConversionError *error);

/* Packet-level join of proxy range movies stored in the input descriptor, in
   the listed order, into one MP4 on the output descriptor. Every packet must
   be an intra picture at exactly the next expected time of its range, and
   every range must carry the same decoder configuration. */
typedef struct {
    uint64_t offset;
    uint64_t length;
    uint64_t frames;
    int64_t start_pts;
    int64_t end_pts;
} DeadpanProxySegment;

typedef struct {
    uint64_t input_byte_length;
    uint32_t width;
    uint32_t height;
    uint32_t time_base_num;
    uint32_t time_base_den;
    uint32_t sar_num;
    uint32_t sar_den;
    uint32_t rotation_quarter_turns;
    uint32_t transfer;
    uint32_t primaries;
    uint64_t frames;
    const DeadpanProxySegment *segments;
    uint64_t segment_count;
    uint64_t max_output_bytes;
    uint64_t timeout_ms;
} DeadpanProxyAssembleRequest;

typedef struct {
    uint64_t output_bytes;
    uint64_t packets;
    uint32_t width;
    uint32_t height;
} DeadpanProxyAssembleReport;

int deadpan_proxy_assemble(int input_fd, int output_fd, const DeadpanProxyAssembleRequest *request,
                           DeadpanProxyAssembleReport *report, DeadpanConversionError *error);

#endif
